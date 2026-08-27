"""Density: a preset over `[frame] slots`, and a keypress that changes one running frame.

Three properties are load-bearing here, and each has its own class below.

**Density is a preset, not a second configuration system.** `slots` stays the primitive:
`instance.frame_of` expands a declared level into the very same list an operator could
have written by hand, and an explicit `slots` overrides it. Nothing downstream of
`frame_of` — the launcher, the probe, the doctor row, `layout.panel_argvs` — learns that
presets exist, which is the property that keeps them from having to agree with a second
source of truth.

**The shipped `density` and the shipped `slots` must expand to the same frame.** They are
two ways of asking charter for the default, and a plane where they disagree gives one
answer to `charter.toml` and another to the palette. `ShippedDefaultsAgree` asserts it
mechanically rather than leaving it to be re-checked by eye — see its own docstring for
why that matters more than usual right now.

**A keypress changes the frame, never the file.** `charter.toml` is hand-maintained and
committed; charter's rule is that machine-written config belongs somewhere a machine may
rewrite whole. `LiveOverride` pins that the override lands in the frame's own state
directory and that `cmd_density` touches no `charter.toml` at all.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import (commands_frame, config, inflight, instance, statusline, tui,
                     util)
from charter.frame import builtin_actions, gather, layout, panel, slots, state

from tests._isolation import PersonaIso


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped — `tests/test_frame_state.py`'s own
    helper, repeated rather than imported because a test module importing another test
    module's private helper couples two files that are otherwise independent.

    A made-up number is a guess about the machine rather than a fact about it, and a
    hand-written `-1` is worse than a guess: pid 1 is `launchd`/`init`, which never exits,
    so a fixture named `something-1` reads as a live launcher and makes any assertion that
    depends on the frame being finished unfailable."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


class _Tmux:
    """A recording stand-in for `tmuxctl.run`, answering the two queries `cmd_density`
    actually reads a value out of and returning success for everything else.

    Not `_FakeTmux` from `tests/test_frame_launcher.py`: that one models a whole LAUNCH
    (new-session, hooks, attach, exit codes) and would have to be taught a second life as
    an already-running frame. What a re-layout needs is much smaller — a window size and a
    pane id per split — so this fake is small enough to read in one screen, which is the
    only way a test can prove an ORDER of tmux calls.
    """

    def __init__(self, *, size="200:50", new_panes=("%7", "%8", "%9"),
                 sizes=(), pane_cols=None):
        self.size = size
        #: Window sizes handed out IN ORDER, one per `#{window_width}` query, the LAST one
        #: sticking once the list runs out. A window that is still moving is the whole of
        #: #501, and a fake with one answer cannot express one: `cmd_resize` measures, and
        #: then asks again immediately before it applies, so a two-element list is a
        #: terminal that changed size between those two moments — and a terminal that has
        #: stopped moving keeps answering the size it stopped at, which is why the tail
        #: sticks rather than reverting to *size*.
        self.sizes = list(sizes)
        #: What tmux answers for `#{pane_width}` (#510). ``None`` means "declines to say",
        #: which is an empty stdout and the case `_variable_pane_cols` falls back to
        #: `layout.repos_cols`' derivation for — the answer every test written before #510
        #: was asserting against, so leaving this alone keeps them exercising it.
        self.pane_cols = pane_cols
        self.new_panes = list(new_panes)
        self.calls: list[list[str]] = []

    def __call__(self, action, argv, *, env=None, timeout=None, report=True):
        self.calls.append(list(argv))
        out = ""
        if commands_frame._PANE_WIDTH_FORMAT in argv:
            out = "" if self.pane_cols is None else str(self.pane_cols)
        elif "display-message" in argv:
            if self.sizes:
                self.size = self.sizes.pop(0)
            out = self.size
        elif "split-window" in argv:
            out = self.new_panes.pop(0) if self.new_panes else ""
        return subprocess.CompletedProcess(argv, 0, stdout=out, stderr="")

    def where(self, *words: str) -> list[int]:
        """Indices of every recorded call containing all of *words* — how an ORDER is
        asserted here, since the thing being pinned is which tmux command ran before
        which, not how many times each ran."""
        return [i for i, c in enumerate(self.calls)
                if all(any(w == part for part in c) for w in words)]


class ShippedDefaultsAgree(unittest.TestCase):
    """`[frame] density` and `[frame] slots` are two ways of asking for the shipped frame.

    **This is the guard, and it is deliberately mechanical.** If the two ever disagree, an
    operator gets one frame from writing nothing at all and a different one from writing
    `density = "<the shipped default>"` — silently, because nothing else anywhere compares
    them.

    It has already earned its keep once. #386 raised the shipped `slots` to all four edges
    while this change was adding `full` as the preset naming those same edges; the two
    landed in separate branches, and on the merge this class went red until the shipped
    `density` moved `normal` -> `full`. That is the whole point of asserting it rather than
    remembering it: the flip happened at merge time instead of becoming a divergence
    nobody saw. The invariant is now three things, because slot lists alone proved too
    weak — the same edges, in the same ORDER (order is geometry, see
    `test_full_puts_the_strips_before_the_side_panels`), at the same verbosity (`minimal`
    and `normal` share a slot list and differ only in how much they say).
    """

    def test_the_shipped_density_expands_to_exactly_the_shipped_slots(self):
        level = instance.FRAME_DEFAULTS["density"]
        self.assertIn(level, instance.FRAME_DENSITY,
                      "the shipped density must name a level that exists")
        self.assertEqual(instance.FRAME_DENSITY[level]["slots"],
                         instance.FRAME_DEFAULTS["slots"],
                         f"the shipped `density = {level!r}` and the shipped `slots` "
                         f"describe different frames — see this class's docstring")

    def test_the_shipped_density_draws_everything_a_panel_has(self):
        """The slot-list invariant above cannot see this: `minimal` and `normal` expand to
        the SAME two edges and differ only in how much each panel says, so a shipped
        default of `minimal` would satisfy it while quietly shipping a terse frame to
        everyone. Charter ships the frame saying what it has; asking for less is the
        operator's choice."""
        self.assertEqual(
            instance.verbosity_for(instance.FRAME_DEFAULTS["density"]), "normal")

    def test_full_is_every_edge_charter_draws(self):
        """#387's own words, re-derived by #488 rather than restated: `full` means every
        edge there IS, so retiring `left` had to move both this table and `FRAME_SLOTS`
        together or one of them would have been lying. Pinned separately from the
        invariant above so that a change making `full` mean something narrower cannot
        satisfy the agreement test by shrinking `full` to match a smaller `slots`
        default."""
        self.assertEqual(sorted(instance.FRAME_DENSITY["full"]["slots"]),
                         sorted(instance.FRAME_SLOTS))

    def test_full_puts_the_strips_before_the_side_panel(self):
        """The ORDER is geometry, not a reading order, and a sorted list would silently
        narrow the row that matters most. `layout.panel_argvs` splits in list order off
        the harness pane, so a `bottom` listed after `right` gets only the width it left
        behind — measured against tmux 3.7c at 200x50 (#386): 200 columns this way,
        **177** with the side panel first. `bottom` carries the one alert, the command
        that fixes it, and (since #488) the repo table whose four columns want 95 of
        them, and `_bottom` drops whole fields when it runs out of width.

        Asserted as index comparisons rather than as the literal list, so it says what is
        load-bearing (strips before sides) rather than freezing a list that may gain a
        fourth slot."""
        order = instance.FRAME_DENSITY["full"]["slots"]
        for strip in ("top", "bottom"):
            for side in ("right",):
                with self.subTest(strip=strip, side=side):
                    self.assertLess(order.index(strip), order.index(side),
                                    f"{strip!r} must be split before {side!r} — see "
                                    f"this test's docstring for the measurement")

    def test_no_level_names_the_retired_sidebar(self):
        """#488's other half. `FRAME_SLOTS` filtering `left` out of an operator's own
        list is not enough on its own — a preset is charter's own constant and reaches
        `_drawable_slots` without passing that filter, so a `full` still naming `left`
        would split a 22-column pane for a slot with no renderer on every launch. Both
        registries have to move together, and this is what makes that mechanical."""
        for level, spec in instance.FRAME_DENSITY.items():
            with self.subTest(level=level):
                self.assertNotIn("left", spec["slots"])

    def test_every_level_agrees_with_the_shipped_slots_about_order(self):
        """The shipped `slots` list and any level that names the same edges must split
        them in the same order, or `charter.toml`'s two ways of asking for one frame
        produce two different geometries."""
        shipped = instance.FRAME_DEFAULTS["slots"]
        for level, spec in instance.FRAME_DENSITY.items():
            with self.subTest(level=level):
                common = [s for s in shipped if s in spec["slots"]]
                self.assertEqual([s for s in spec["slots"] if s in shipped], common)

    def test_minimal_is_the_two_strips_and_literally_nothing_else(self):
        """#387's own words, re-derived by #488 and again by #515 — and this time they
        are literally true again. #488 made `bottom` variable-height, so "one-line top
        and bottom" stopped being a row count and became a verbosity. #515 gave the table
        its own slot, so the two strips are one row each once more (`SLOT_SIZE`), and
        `minimal` is what it says: those two and nothing that costs rows by the repo.

        All three halves asserted — the slot list, the verbosity, and the row counts —
        because a `minimal` that quietly grew `repos` back would still be terse and still
        name two strips first, and would cost the harness a whole component's worth of
        rows plus a border to show four repos."""
        self.assertEqual(instance.FRAME_DENSITY["minimal"]["slots"], ["top", "bottom"])
        self.assertEqual(instance.FRAME_DENSITY["minimal"]["verbosity"], "terse")
        self.assertNotIn("repos", instance.FRAME_DENSITY["minimal"]["slots"])
        self.assertEqual(layout.SLOT_SIZE["top"], 1)
        self.assertEqual(layout.SLOT_SIZE["bottom"], 1)
        self.assertEqual(
            layout.slot_sizes(["top", "bottom"], window_rows=50, content_rows=9),
            {"top": 1, "bottom": 1},
            "minimal must not have a slot whose height moves with the content")

    def test_every_level_expands_to_slots_charter_actually_accepts(self):
        """A level naming a slot outside `FRAME_SLOTS` would be filtered out of an
        operator's own `slots` list as a typo and then smuggled back in by a preset."""
        for level, spec in instance.FRAME_DENSITY.items():
            with self.subTest(level=level):
                for slot in spec["slots"]:
                    self.assertIn(slot, instance.FRAME_SLOTS)
                self.assertIn(spec["verbosity"], ("terse", "normal"))


class DensityResolves(unittest.TestCase):
    """`instance.frame_of`: what a `[frame] density` in charter.toml actually does."""

    def test_a_declared_level_expands_into_the_slot_list(self):
        f = instance.frame_of({"frame": {"density": "full"}})
        self.assertEqual(f["density"], "full")
        self.assertEqual(f["slots"], instance.FRAME_DENSITY["full"]["slots"])

    def test_a_narrower_level_expands_too(self):
        """The other direction, so a stub that always returned `full`'s list — which
        would pass the test above — fails here."""
        f = instance.frame_of({"frame": {"density": "minimal"}})
        self.assertEqual(f["slots"], ["top", "bottom"])

    def test_an_explicit_slots_list_overrides_a_declared_density(self):
        """#387: "an explicit `slots` overrides it". `slots` is the primitive; a preset
        cannot outrank the thing it is a preset for."""
        f = instance.frame_of({"frame": {"density": "full", "slots": ["bottom"]}})
        self.assertEqual(f["slots"], ["bottom"])
        self.assertEqual(f["density"], "full",
                         "the level is still resolved — it is what sets verbosity")

    def test_a_slots_list_that_is_entirely_typos_leaves_the_density_in_charge(self):
        """`slots = ["sideway"]` filters to nothing, so nothing was actually asked for.
        Falling back to the shipped default there would ignore a density the operator
        DID write successfully, on account of a key they got wrong."""
        f = instance.frame_of({"frame": {"density": "full", "slots": ["sideway"]}})
        self.assertEqual(f["slots"], instance.FRAME_DENSITY["full"]["slots"])

    def test_no_density_at_all_leaves_the_shipped_slots_untouched(self):
        """The expansion is conditional on the key being DECLARED, which is what keeps
        the shipped `slots` default load-bearing rather than dead: a change to that list
        alone must still change the shipped frame.

        **The level to sabotage is READ, never named.** An earlier version of this test
        hard-coded `"normal"` — true when it was written, and silently vacuous the moment
        #386 raised the shipped `slots` to all four edges and the shipped `density`
        followed it to `full`: patching a level nothing defaults to changes nothing, so
        the test passed against unconditional expansion too. Caught by the mutation run,
        not by reading. Reading `FRAME_DEFAULTS["density"]` means the sabotage always
        lands on the level the default path actually takes, whatever it becomes next.
        """
        shipped = instance.FRAME_DEFAULTS["density"]
        with mock.patch.dict(instance.FRAME_DENSITY,
                             {shipped: {"slots": ["left"], "verbosity": "normal"}}):
            f = instance.frame_of({"frame": {"mouse": True}})
        self.assertEqual(f["slots"], instance.FRAME_DEFAULTS["slots"],
                         "an undeclared density expanded — the shipped `slots` default "
                         "is now dead code, and #386's own change to it does nothing")

    def test_an_unknown_level_degrades_to_the_shipped_one(self):
        f = instance.frame_of({"frame": {"density": "enormous"}})
        self.assertEqual(f["density"], instance.FRAME_DEFAULTS["density"])
        self.assertEqual(f["slots"], instance.FRAME_DEFAULTS["slots"])

    def test_a_non_string_density_does_not_raise(self):
        """`value in FRAME_DENSITY` raises `TypeError` for an unhashable value, and this
        module is imported by every command including `charter --version` — the same trap
        `_HOTKEY_RE`'s own type check exists for. A TOML array and a table are both
        writable by hand and both unhashable."""
        for bad in [["full"], {"level": "full"}, 3, True, None]:
            with self.subTest(density=bad):
                f = instance.frame_of({"frame": {"density": bad}})
                self.assertEqual(f["density"], instance.FRAME_DEFAULTS["density"])
                self.assertEqual(f["slots"], instance.FRAME_DEFAULTS["slots"])

    def test_density_slots_hands_out_a_copy(self):
        """A caller patches the resolved config (`mock.patch.dict(config.FRAME, ...)`)
        and the launcher filters the list it gets back; handing out the table's own list
        would let either edit the preset for the life of the process."""
        got = instance.density_slots("full")
        got.append("sideway")
        self.assertNotIn("sideway", instance.FRAME_DENSITY["full"]["slots"])

    def test_verbosity_for_answers_normal_for_anything_it_does_not_know(self):
        """Not a defensive case: a frame's live override is read off disk, and a frame
        launched by an older charter has no file there at all.

        Asserted against the LITERAL `"normal"`, never `instance.DEFAULT_VERBOSITY`. An
        earlier version compared the function's answer to the constant the function
        returns, which is a tautology: setting `DEFAULT_VERBOSITY = "terse"` — shipping
        every unknown level as a terse frame — passed the whole suite. A test whose
        expected value is taken from the thing under test cannot fail."""
        self.assertEqual(instance.verbosity_for("minimal"), "terse")
        self.assertEqual(instance.DEFAULT_VERBOSITY, "normal",
                         "the fallback for an unknown level is a FULL panel, not a terse "
                         "one — a frame charter cannot make sense of must not silently "
                         "show less")
        for unknown in [None, "", "enormous", 7]:
            with self.subTest(level=unknown):
                self.assertEqual(instance.verbosity_for(unknown), "normal")


class VerbosityIsReadLive(PersonaIso, unittest.TestCase):
    """`slots.verbosity` — the frame's own recorded level first, the configured one
    behind it. The order IS the "overrides for the running frame only" rule."""

    def setUp(self):
        super().setUp()
        self.fid = f"vb-{_a_dead_pid()}"

    def test_the_configured_level_is_used_when_nothing_was_recorded(self):
        with mock.patch.dict(config.FRAME, {"density": "minimal"}):
            self.assertEqual(slots.verbosity(self.fid), "terse")

    def test_a_recorded_override_beats_the_configured_level(self):
        state.record_density(self.fid, "minimal")
        with mock.patch.dict(config.FRAME, {"density": "full"}):
            self.assertEqual(slots.verbosity(self.fid), "terse")

    def test_a_corrupt_recorded_level_degrades_to_the_configured_one(self):
        """The file is on disk, so a truncated write or a hand edit reaches here. It must
        read like an unknown level in charter.toml does, not like "no panels"."""
        d = state.frame_dir(self.fid, create=True)
        (d / "density").write_text("enormous\n")
        with mock.patch.dict(config.FRAME, {"density": "minimal"}):
            self.assertEqual(slots.verbosity(self.fid), "terse")


class TerseSaysLess(PersonaIso, unittest.TestCase):
    """What `verbosity == "terse"` actually costs each panel. Every assertion is a
    comparison against the SAME panel at `normal`, so a renderer that simply broke would
    not pass by rendering less of nothing."""

    def setUp(self):
        super().setUp()
        self.fid = f"tr-{_a_dead_pid()}"

    def _render(self, slot, level):
        state.record_density(self.fid, level)
        with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((120, 40))):
            return slots.render(slot, self.fid)

    def test_top_drops_the_charter_version_and_keeps_the_workspace(self):
        from charter import __version__
        normal = self._render("top", "normal")
        terse = self._render("top", "minimal")
        self.assertIn(__version__, normal)
        self.assertNotIn(__version__, terse)
        self.assertIn("⬢", terse, "the workspace mark is the answer top exists to give")

    def test_top_drops_the_dev_chip_too_when_it_drops_the_version(self):
        """#457: the dev-channel chip follows the version rather than getting a rule of
        its own — it is a fact ABOUT the version (`_dev_chip`'s own docstring), so a
        density that already decided the version does not earn its columns must not
        keep half of it around. On a plane declaring `[update] channel = "dev"`, `normal`
        carries both the version and the word `dev`; `minimal` carries neither."""
        with mock.patch.object(config, "UPDATE", {"channel": "dev"}):
            normal = self._render("top", "normal")
            terse = self._render("top", "minimal")
        self.assertIn("dev", normal)
        self.assertNotIn("dev", terse)

    def test_bottom_keeps_exactly_one_field(self):
        with mock.patch("charter.statusline._todo_count", return_value=3), \
             mock.patch("charter.statusline._alerts",
                        return_value=["⚠ reinit: charter ws reinit needed"]):
            normal = self._render("bottom", "normal")
            terse = self._render("bottom", "minimal")
        self.assertIn("·", normal, "a healthy `normal` row carries several fields")
        self.assertNotIn("·", terse)
        self.assertIn("reinit", terse,
                      "the field that survives must be the highest-priority one")

    def test_bottom_is_never_blank_on_a_quiet_plane(self):
        """The one-field cap must not produce an empty row when nothing is wrong: the
        todo count is unconditional, so it is what is left."""
        with mock.patch("charter.statusline._alerts", return_value=[]), \
             mock.patch("charter.statusline._session_news", return_value=[]), \
             mock.patch("charter.statusline._todo_count", return_value=0):
            terse = self._render("bottom", "minimal")
        self.assertIn("todo", terse)

    def test_the_tables_pane_shows_fewer_repos_and_says_how_many_it_hid(self):
        """#488 moved the repo table off the retired `left` sidebar and `terse` came with
        it; #515 moved it again, into `repos`. The level's meaning is unchanged — it is
        the slot with rows to give, so "less" means fewer of them — and the arithmetic
        lost its `+ 1`, because the attention row is another pane's now. Asserted as line
        COUNTS against the same panel at `normal`, so a renderer that merely broke cannot
        pass by drawing less of nothing."""
        rows = [{"name": f"repo{i}", "branch": "main", "dirty": False,
                 "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None,
                 "change": None, "sigil": "", "current": False, "worktree_count": 0}
                for i in range(9)]
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": rows, "worktrees": []})
        normal = self._render("repos", "normal")
        terse = self._render("repos", "minimal")
        self.assertEqual(len(normal.split("\n")), 1 + 9)
        self.assertEqual(len(terse.split("\n")), 1 + slots._TERSE_ROWS)
        self.assertIn("more", terse, "a panel showing less must say how much less")

    def test_the_terse_table_still_keeps_the_repo_that_needs_attention(self):
        """The rows that survive are `_pick_rows`' ranked subset, not the first four —
        the exact lesson `statusline.py` paid for in production (an unranked slice of 18
        clones showed thirteen clean repos and hid the dirty one you were standing in).
        The dirty repo is placed LAST in the cache so position cannot be what saved it."""
        rows = [{"name": f"repo{i}", "branch": "main", "dirty": False,
                 "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None,
                 "change": None, "sigil": "", "current": False, "worktree_count": 0}
                for i in range(9)]
        rows[-1] = dict(rows[-1], name="the-dirty-one", dirty=True, tracked_dirty=True)
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": rows, "worktrees": []})
        terse = self._render("repos", "minimal")
        self.assertIn("the-dirty-one", terse)
        # `_pick_rows` re-sorts the CHOSEN set back into cache order, so the two clean
        # repos that share the last rows are still `repo0`/`repo1` — what distinguishes
        # ranked from unranked is `repo2`, which a plain head-of-list slice would have
        # kept in place of the dirty one.
        self.assertNotIn("repo2", terse, "an unranked slice would have kept repo2")

    def test_the_terse_table_drops_piece_rows_before_repo_rows(self):
        """Pieces are DETAIL under a repo that still has its own row, so losing them
        costs no repo its line — the trade `_table_lines` makes when the budget is
        short, and the same one the wide table's own `wt_budget` makes."""
        gather.save(self.fid, {
            "gathered_at": 0.0, "workspace": "w", "current_repo": None,
            "repos": [{"name": "solo", "branch": "main", "dirty": False,
                       "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None,
                       "change": None, "sigil": "", "current": True,
                       "worktree_count": 6}],
            "worktrees": [{"name": f"piece-{i}", "repo": "solo", "branch": "wip",
                           "dirty": False, "tracked_dirty": False, "ahead": 0,
                           "behind": 0, "ci": None, "change": None, "sigil": "",
                           "current": False, "worktree_count": 0}
                          for i in range(6)]})
        normal = self._render("repos", "normal")
        terse = self._render("repos", "minimal")
        self.assertIn("piece-5", normal)
        self.assertNotIn("piece-5", terse)
        self.assertIn("solo", terse, "the repo keeps its row whatever its pieces lose")

    def test_right_shows_fewer_personas_and_says_how_many_it_hid(self):
        """`+ 1` on both counts is the `personas` heading #516 added — it is chrome the
        density does not buy back, because a column of names with no title is what the
        heading exists to fix and one row is not what `minimal` is short of."""
        cells = [statusline.PersonaChip(f"p{i}", f"▫ p{i}", "") for i in range(9)]
        with mock.patch("charter.statusline._persona_chip_cells", return_value=cells):
            normal = self._render("right", "normal")
            terse = self._render("right", "minimal")
        self.assertEqual(len(normal.split("\n")), 9 + 1)
        self.assertEqual(len(terse.split("\n")), slots._TERSE_ROWS + 1)
        self.assertIn("more", terse)
        self.assertIn("personas 9", tui.strip_ansi(terse),
                      "the heading must count the personas, not the rows that fit")


class TheSpinnerRunsOnlyWhileWorkDoes(PersonaIso, unittest.TestCase):
    """`slots._inflight_field` — the only moving thing in the frame.

    Idle stillness is the property, not a nicety: a spinner that keeps turning with
    nothing behind it is a panel repainting several times a second forever, which is
    exactly what "nothing may cost anything at idle" rules out.
    """

    def _record(self, agent="worker", *, age=0.0):
        d = config.STATE_DIR / "dispatch-inflight"
        d.mkdir(parents=True, exist_ok=True)
        p = d / f"{agent}.{age}.json"
        p.write_text(json.dumps({"agent": agent, "ts": time.time() - age}))
        return p

    def test_nothing_in_flight_draws_nothing_at_all(self):
        """Empty, not `⠋ 0 running`: an empty field is DROPPED by `_fit_fields`, which is
        what makes an idle bottom row identical to the one that shipped before this."""
        self.assertEqual(slots._inflight_field(), "")

    def test_a_running_dispatch_is_counted_and_animated(self):
        self._record("alpha")
        self._record("beta")
        field = slots._inflight_field()
        self.assertIn("2 running", field)
        self.assertTrue(any(f in field for f in slots.SPINNER),
                        f"no spinner frame in {field!r}")

    def test_a_presumed_dead_record_is_reported_but_never_animated(self):
        """`inflight` keeps a record for a full day past the presumed-dead threshold so a
        stuck dispatch stays visible. Animating it would claim progress that stopped
        thirty minutes ago — and would spin a panel for that whole day."""
        self._record("stuck", age=inflight.PRESUMED_DEAD_SECONDS + 60)
        field = slots._inflight_field()
        self.assertIn("1 stalled", field)
        self.assertNotIn("running", field)
        self.assertFalse(any(f in field for f in slots.SPINNER), field)

    def test_the_spinner_advances_with_the_clock(self):
        """Stateless by construction (`SPINNER_PERIOD` off `time.monotonic`), so nothing
        has to own or reset it — and two panels painting at the same instant necessarily
        draw the same frame."""
        first = slots.spinner_frame(0.0)
        later = slots.spinner_frame(slots.SPINNER_PERIOD)
        self.assertNotEqual(first, later)
        self.assertEqual(first,
                         slots.spinner_frame(slots.SPINNER_PERIOD * len(slots.SPINNER)))

    def test_every_spinner_frame_is_one_column_wide(self):
        """A frame two columns wide would make the whole bottom row jump sideways ten
        times a second — worse than no spinner, and invisible to a test that only checks
        the count."""
        for f in slots.SPINNER:
            with self.subTest(frame=f):
                self.assertEqual(tui.width(f), 1)


class OnlyTheAnimatedSlotAnimates(PersonaIso, unittest.TestCase):
    """`slots.ANIMATED` must name exactly the renderers whose output changes on its own.

    **This is a cost property, not a tidiness one.** `panel._watch` runs one process per
    slot, so an unscoped "is work in flight" repaints all four at `panel.TICK` for the
    whole length of a dispatch — three of them redrawing byte-identical output. Measured
    on this project (8 personas, 6 repos), one `render("right")` costs 4 816µs, because
    `statusline._persona_chips` asks `persona.is_draft`, `structural_errors`, `_mem_count`
    twice and `_vault_dot` per persona — a helper whose own docstring says it "renders on
    every single turn", written for once a turn rather than five times a second. At 5Hz
    that pane alone is ~2.4% of a core, during the window `panel.py`'s docstring calls
    quiet.

    Asserted behaviourally rather than by reading the set: each slot is rendered twice, at
    two clock readings a spinner frame apart, with a record in flight. Output that differs
    means the slot animates. A renderer that gains a spinner without joining `ANIMATED`,
    or keeps its membership after losing one, is red — which a hand-maintained set of
    names cannot be on its own.
    """

    def setUp(self):
        super().setUp()
        self.fid = f"an-{_a_dead_pid()}"
        d = config.STATE_DIR / "dispatch-inflight"
        d.mkdir(parents=True, exist_ok=True)
        (d / "w.1.json").write_text(json.dumps({"agent": "w", "ts": time.time()}))
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": [], "worktrees": []})

    def _render_at(self, slot, now):
        with mock.patch("charter.frame.slots.time.monotonic", return_value=now), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((120, 40))):
            return slots.render(slot, self.fid)

    def test_exactly_the_slots_in_ANIMATED_change_with_the_clock_alone(self):
        moving = set()
        for slot in slots.SLOTS:
            with self.subTest(slot=slot):
                first = self._render_at(slot, 0.0)
                later = self._render_at(slot, slots.SPINNER_PERIOD * 3)
                if first != later:
                    moving.add(slot)
        self.assertEqual(moving, set(slots.ANIMATED),
                         f"`slots.ANIMATED` says {sorted(slots.ANIMATED)} but the "
                         f"renderers that actually move are {sorted(moving)}")

    def test_a_non_animated_slot_never_asks_whether_work_is_in_flight(self):
        """The short-circuit in `_watch`, which is what makes a `top`/`left`/`right` panel
        cost exactly what it cost before this feature existed — not merely repaint less."""
        for slot in sorted(set(slots.SLOTS) - set(slots.ANIMATED)):
            with self.subTest(slot=slot):
                with mock.patch("charter.frame.panel._running",
                                side_effect=AssertionError("asked, and must not")), \
                     mock.patch("charter.frame.panel._paint"):
                    panel._watch(slot, self.fid, once=True)

    def test_the_animated_slot_does_ask(self):
        """The other direction, so the test above cannot be satisfied by never asking."""
        with mock.patch("charter.frame.panel._running", return_value=1) as asked, \
             mock.patch("charter.frame.panel._paint"):
            panel._watch("bottom", self.fid, once=True)
        asked.assert_called_once()


class IdleCostsOneStat(PersonaIso, unittest.TestCase):
    """`panel._running` — how the panel learns work is in flight without paying for it.

    The expensive answer (`inflight.live_records`: opendir, readdir, a JSON parse per
    record) is behind a single `stat` of the tracker's directory, whose mtime moves
    whenever a record is created or removed. These tests assert the CALLS, not the
    wall-clock time, because a timing assertion on a shared CI box measures the box.
    """

    def test_an_idle_panel_never_reads_the_records_twice(self):
        cache = panel._new_inflight_cache()
        panel._running(cache)
        with mock.patch("charter.inflight.live_records",
                        side_effect=AssertionError("idle must not re-read")) as reader:
            for _ in range(20):
                self.assertEqual(panel._running(cache), 0)
            reader.assert_not_called()

    def test_an_idle_tick_costs_exactly_one_stat_and_nothing_else(self):
        """The number #387 asks to be MEASURED rather than asserted, pinned as a syscall
        budget so it cannot quietly grow: at idle the gate is one `stat` of the tracker's
        own directory, no file opened, and no process started. Measured alongside this on
        macOS/APFS: ~4.8µs per tick, against the ~26µs a panel already spends on its
        version poll, at `panel.TICK` = 5 ticks a second.

        **Counting `Path.stat` alone was not a budget.** An earlier version patched
        `pathlib.Path.stat` and asserted one call, which constrains only calls that go
        through that one method: an `open()` per tick survived it, and so did an `os.stat`
        plus a `subprocess.run` per tick. Worse, it was interpreter-dependent — on Python
        3.14 `Path.exists()` no longer routes through `Path.stat`, so the very
        `live_records` read this exists to keep out of the idle path was invisible to the
        counter, and deleting the whole gate still passed. CI runs 3.11 through 3.14, so
        "it would have been caught on 3.11" is an accident of pathlib internals, not a
        test. All three primitives are counted now, at the bottom (`os.stat`,
        `builtins.open`, `subprocess.run`), which is where every higher-level spelling —
        `Path.stat`, `Path.exists`, `read_text`, `glob` — must eventually arrive.

        Counted rather than timed, because a wall-clock assertion on a shared CI box
        measures the box."""
        import builtins
        import subprocess as _sp
        cache = panel._new_inflight_cache()
        panel._running(cache)          # prime: the first tick always reads
        real_stat, real_open, real_run = os.stat, builtins.open, _sp.run
        stats, opens, runs = [], [], []

        def c_stat(*a, **k):
            stats.append(a)
            return real_stat(*a, **k)

        def c_open(*a, **k):
            opens.append(a)
            return real_open(*a, **k)

        def c_run(*a, **k):
            runs.append(a)
            return real_run(*a, **k)

        with mock.patch("os.stat", c_stat), mock.patch("builtins.open", c_open), \
             mock.patch("subprocess.run", c_run):
            panel._running(cache)
        self.assertEqual(len(stats), 1, stats)
        self.assertEqual(opens, [], "an idle tick opened a file")
        self.assertEqual(runs, [], "an idle tick started a process")

    def test_a_new_record_is_noticed(self):
        """The other half: a cache that never refreshed would pass the test above and
        make the spinner never start."""
        cache = panel._new_inflight_cache()
        self.assertEqual(panel._running(cache), 0)
        d = config.STATE_DIR / "dispatch-inflight"
        d.mkdir(parents=True, exist_ok=True)
        (d / "w.1.json").write_text(json.dumps({"agent": "w", "ts": time.time()}))
        self.assertEqual(panel._running(cache), 1)

    def test_a_cleared_record_returns_the_panel_to_stillness(self):
        d = config.STATE_DIR / "dispatch-inflight"
        d.mkdir(parents=True, exist_ok=True)
        rec = d / "w.1.json"
        rec.write_text(json.dumps({"agent": "w", "ts": time.time()}))
        cache = panel._new_inflight_cache()
        self.assertEqual(panel._running(cache), 1)
        rec.unlink()
        self.assertEqual(panel._running(cache), 0)

    def test_a_presumed_dead_record_stops_the_animation_without_any_file_changing(self):
        """The one way this answer changes with no file touched — which is why the mtime
        alone is not the whole gate. A record that crosses the threshold while the panel
        watches must stop the spinner, or a killed dispatch animates for a day.

        One patch, not two: `inflight` and `panel` both do `import time`, so
        `charter.inflight.time` and `charter.frame.panel.time` are the SAME module object
        and patching `time.time` through either name moves both clocks at once — which is
        what this test needs, since the deadline is computed in one and compared in the
        other."""
        d = config.STATE_DIR / "dispatch-inflight"
        d.mkdir(parents=True, exist_ok=True)
        (d / "w.1.json").write_text(json.dumps({"agent": "w", "ts": time.time()}))
        cache = panel._new_inflight_cache()
        self.assertEqual(panel._running(cache), 1)
        later = time.time() + inflight.PRESUMED_DEAD_SECONDS + 60
        with mock.patch("charter.inflight.time.time", return_value=later):
            self.assertEqual(panel._running(cache), 0)

    def test_a_tracker_that_cannot_be_read_is_stillness_not_a_crash(self):
        """This runs in a panel's loop, where an exception ends the pane."""
        cache = panel._new_inflight_cache()
        with mock.patch("charter.inflight.stamp", side_effect=OSError("gone")):
            self.assertEqual(panel._running(cache), 0)

    def test_the_stamp_is_none_when_nothing_has_ever_dispatched(self):
        self.assertIsNone(inflight.stamp())


class TheLoopPaintsForTheSpinner(PersonaIso, unittest.TestCase):
    """`panel._tick` gains a third reason to repaint, and it must not gain a fourth by
    accident: an idle panel repainting every tick is the cost this whole design avoids."""

    def test_animating_paints_even_with_no_version_change_and_no_resize(self):
        fid = f"an-{_a_dead_pid()}"
        seen = state.version(fid)
        with mock.patch("charter.frame.panel._paint") as paint:
            panel._tick({"flag": False}, seen, "bottom", fid, animating=True)
        paint.assert_called_once_with("bottom", fid)

    def test_not_animating_is_the_default_and_paints_nothing(self):
        fid = f"an-{_a_dead_pid()}"
        seen = state.version(fid)
        with mock.patch("charter.frame.panel._paint") as paint:
            panel._tick({"flag": False}, seen, "bottom", fid)
        paint.assert_not_called()


class TheLoopAsksBeforeItAnimates(PersonaIso, unittest.TestCase):
    """`_watch` is what connects `_running` to `_tick`, and nothing else does.

    Pinned separately from both because the wiring is its own failure: a loop that never
    passes `animating` leaves a perfectly correct `_running` and a perfectly correct
    `_tick` with a spinner that never turns, and every test of either half stays green.
    """

    def setUp(self):
        super().setUp()
        self.fid = f"wt-{_a_dead_pid()}"

    def _watch_once(self):
        with mock.patch("charter.frame.panel._tick", return_value="") as tick:
            panel._watch("bottom", self.fid, once=True)
        return tick.call_args

    def test_a_record_in_flight_reaches_the_tick_as_animating(self):
        d = config.STATE_DIR / "dispatch-inflight"
        d.mkdir(parents=True, exist_ok=True)
        (d / "w.1.json").write_text(json.dumps({"agent": "w", "ts": time.time()}))
        self.assertIs(self._watch_once().kwargs["animating"], True)

    def test_an_idle_loop_does_not_animate(self):
        """The half that stops the test above from passing against a loop hardcoded to
        animate — which would repaint every panel five times a second, forever, which is
        the one thing this feature may not do."""
        self.assertIs(self._watch_once().kwargs["animating"], False)


class PanesAreRememberedForTheFrame(PersonaIso, unittest.TestCase):
    """`state.record_panes`/`state.panes` — the only record of which pane draws which SLOT.

    tmux cannot be asked later: `list-panes` reports ids and geometry, nothing that says
    which pane charter meant as `left`, and inferring it from position is the pane-index
    trap `frame/layout.py`'s module docstring measures.

    The HARNESS pane is deliberately not in here — `state.record_harness_pane` owns that
    one fact for ADR 0019's `is_live`, and a second copy would be one fact free to
    disagree with itself. `cmd_density` reads it from there.
    """

    def setUp(self):
        super().setUp()
        self.fid = f"pn-{_a_dead_pid()}"

    def test_a_recorded_map_reads_back(self):
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
        self.assertEqual(state.panes(self.fid), {"top": "%1", "bottom": "%2"})

    def test_a_frame_with_no_record_answers_empty_rather_than_raising(self):
        self.assertEqual(state.panes(self.fid), {})

    def test_a_corrupt_file_answers_empty(self):
        d = state.frame_dir(self.fid, create=True)
        (d / "panes").write_text("{not json")
        self.assertEqual(state.panes(self.fid), {})

    def test_non_string_ids_are_dropped_rather_than_handed_to_a_tmux_argv(self):
        d = state.frame_dir(self.fid, create=True)
        (d / "panes").write_text('{"top": 7, "bottom": "%2"}')
        self.assertEqual(state.panes(self.fid), {"bottom": "%2"})

    def test_identity_drops_values_that_are_not_strings(self):
        """This is JSON on disk and every value goes straight into a tmux `-e NAME=VALUE`
        argv element, so a hand edit or a charter that wrote a different shape reaches
        here. Same guard `panes` keeps, for the same reason."""
        state.record_identity(self.fid, {"CHARTER_ROOT": "/p"})
        (state.frame_dir(self.fid) / "identity").write_text(
            '{"CHARTER_ROOT": "/p", "CHARTER_WORKSPACE": 7, "CHARTER_HARNESS": null}')
        self.assertEqual(state.identity(self.fid), {"CHARTER_ROOT": "/p"})

    def test_identity_answers_empty_for_a_frame_that_never_recorded_one(self):
        self.assertEqual(state.identity(f"never-{_a_dead_pid()}"), {})

    def test_a_launch_clears_a_density_it_inherited_with_a_recycled_pid(self):
        """#383's bill, applied to the two files this change adds. A frame id is
        `<workspace>-<launcher pid>`; `reap` keeps a directory while that pid is live, and
        on a launch it is live because it is the launcher's own — so a launcher landing on
        a pid an earlier launcher for the same workspace used adopts that frame's whole
        directory. A `density` inherited that way is another session's keypress silently
        overriding this plane's `[frame] density`, which is exactly what "for the running
        frame only" promises cannot happen."""
        state.record_density(self.fid, "minimal")
        state.record_panes(self.fid, panels={"top": "%1"})
        state.clear_shape(self.fid)
        self.assertIsNone(state.density(self.fid))
        self.assertEqual(state.panes(self.fid), {})

    def test_clearing_a_frame_that_never_recorded_anything_is_a_no_op(self):
        """Never creates, never raises: the ordinary first launch for a workspace has no
        directory here at all and must not mint one just to empty it."""
        state.clear_shape(f"never-{_a_dead_pid()}")

    def test_a_launch_writes_the_map_down(self):
        """The map has to exist before a density change can use it, and the only place it
        can be captured is where the splits happen."""
        fake = _Tmux(new_panes=["%11", "%12"])
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            commands_frame._draw_panels("charter", slots=["top", "bottom"], fid=self.fid,
                                        harness_pane="%0", env=None, v=(3, 7))
        self.assertEqual(state.panes(self.fid), {"top": "%11", "bottom": "%12"})


class LiveOverride(PersonaIso, unittest.TestCase):
    """`cmd_density` — the keypress half. Changes one running frame; writes no config."""

    def setUp(self):
        super().setUp()
        self.fid = f"dn-{_a_dead_pid()}"
        self.enterContext(mock.patch.dict(os.environ,
                                          {"CHARTER_SESSION_ID": self.fid}))
        self.enterContext(mock.patch("charter.frame.tmuxctl.version",
                                     return_value=(3, 7)))
        state.record_harness_pane(self.fid, "%0")
        state.record_panes(self.fid,
                           panels={"top": "%1", "bottom": "%2", "repos": "%5"})

    def _run(self, level, fake=None):
        fake = fake or _Tmux()
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            rc = commands_frame.cmd_density(SimpleNamespace(level=level))
        return rc, fake

    def test_it_records_the_level_for_this_frame_only(self):
        rc, _ = self._run("minimal")
        self.assertEqual(rc, 0)
        self.assertEqual(state.density(self.fid), "minimal")

    def test_it_never_writes_charter_toml(self):
        """The rule the whole design turns on: `charter.toml` is hand-maintained, so a
        keypress may not touch it. Proved by making every writer in `instance` raise —
        those two functions are the ONLY code in charter that edits that file."""
        boom = AssertionError("cmd_density wrote to charter.toml")
        with mock.patch("charter.instance._set_key", side_effect=boom), \
             mock.patch("charter.instance.set_locked_version", side_effect=boom), \
             mock.patch("charter.instance.set_default_persona", side_effect=boom):
            rc, _ = self._run("full")
        self.assertEqual(rc, 0)

    def test_growing_to_full_splits_the_slots_that_were_missing(self):
        rc, fake = self._run("full")
        self.assertEqual(rc, 0)
        split_slots = [c[c.index("panel") + 1] for c in fake.calls
                       if "split-window" in c and "panel" in c]
        self.assertEqual(sorted(split_slots), ["right"])
        self.assertEqual(state.panes(self.fid),
                         {"top": "%1", "bottom": "%2", "repos": "%5", "right": "%7"})

    def _repos_split_size(self, size):
        """The `-l` a re-layout hands `split-window` for a `repos` it has to create.

        A density change can ADD `repos` — `minimal` has none, so every step up from it
        is this — and that split's own `-l` comes from `layout.slot_sizes`, not from the
        `_reassert_sizes` that follows it. So it is a second place the window's width has
        to reach, and the correction after it is `report=False` best-effort rather than a
        guarantee.
        """
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
        rows = [{"name": f"repo{i}", "branch": "main", "dirty": False,
                 "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None,
                 "change": None, "sigil": "", "current": False, "worktree_count": 0}
                for i in range(6)]
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": rows, "worktrees": []})
        rc, fake = self._run("normal", _Tmux(size=size))
        self.assertEqual(rc, 0)
        split = next(c for c in fake.calls
                     if "split-window" in c and "repos" in c)
        return int(split[split.index("-l") + 1])

    def test_a_relayout_splits_the_table_for_what_a_wide_window_can_draw(self):
        self.assertEqual(self._repos_split_size("200:50"), 1 + 6)

    def test_a_narrow_window_does_not_get_a_table_pane_at_all(self):
        """#500's other call site, and what #515 changed about it. An 80-column frame
        draws no table, and the pane it used to be given a one-row `-l` for is now not
        split at all (`layout.visible_slots`) — because a bordered rectangle with nothing
        in it reads as "no repos" on a plane that has six.

        The control is the wide case in the test above, so a fix that simply stopped
        splitting `repos` anywhere is red."""
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
        rows = [{"name": f"repo{i}", "branch": "main", "dirty": False,
                 "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None,
                 "change": None, "sigil": "", "current": False, "worktree_count": 0}
                for i in range(6)]
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": rows, "worktrees": []})
        rc, fake = self._run("normal", _Tmux(size="80:50"))
        self.assertEqual(rc, 0)
        self.assertFalse([c for c in fake.calls
                          if "split-window" in c and "repos" in c], fake.calls)

    def test_a_table_split_in_beside_a_surviving_sidebar_gets_the_inset_width(self):
        """Round 3, and the case that says why the order used here is the PANE order and
        not *want*'s. A frame launched with `[frame] slots = ["right", "top", "bottom"]`
        already has a 22-column sidebar; growing it to `full` splits `repos` off a
        harness pane that is therefore already 23 columns narrower than the window.

        `want` at `full` is `["top", "bottom", "repos", "right"]`, whose order says
        `repos` comes before `right` and is full width. That is the order a fresh launch
        would produce, not the order THIS frame's panes are in: `right` is already there
        and is not re-split. So the list handed to `layout.repos_cols` is the surviving
        panes in their recorded order followed by what is about to be split, which is what
        actually happens.

        Asserted at a window width the two answers straddle: 130 columns leaves a full
        table pane 130 wide, and an inset one 107 — both above `statusline._LEFT_W`, so
        both are split, and the sizes differ only because the inset pane holds fewer of
        the table's rows... except it does not, since the rows are the same six. The
        distinguishing case is therefore the width the sidebar pushes BELOW the table's
        own minimum: at 110 the inset pane is 87 wide and is not split at all, while the
        un-inset one is 110 and gets its six rows. A sizer reading the window's width
        would split a seven-row pane there instead — #500's own defect, one slot over.

        The control is the same re-layout with no sidebar surviving, so a fix that simply
        stopped splitting `repos` is red.
        """
        rows = [{"name": f"repo{i}", "branch": "main", "dirty": False,
                 "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None,
                 "change": None, "sigil": "", "current": False, "worktree_count": 0}
                for i in range(6)]
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": rows, "worktrees": []})

        def _split_l(panels):
            state.record_panes(self.fid, panels=panels)
            rc, fake = self._run("full", _Tmux(size="110:50"))
            self.assertEqual(rc, 0)
            cmd = next((c for c in fake.calls
                        if "split-window" in c and "repos" in c), None)
            return None if cmd is None else int(cmd[cmd.index("-l") + 1])

        self.assertIsNone(_split_l({"right": "%3", "top": "%1", "bottom": "%2"}))
        self.assertEqual(_split_l({"top": "%1", "bottom": "%2"}), 1 + 6)

    def test_shrinking_kills_the_panes_it_no_longer_wants(self):
        state.record_panes(self.fid, panels={"top": "%1", "right": "%3", "bottom": "%2",
                                             "repos": "%5"})
        rc, fake = self._run("minimal")
        self.assertEqual(rc, 0)
        killed = {c[c.index("kill-pane") + 2] for c in fake.calls if "kill-pane" in c}
        self.assertEqual(killed, {"%3", "%5"}, fake.calls)
        self.assertEqual(state.panes(self.fid), {"top": "%1", "bottom": "%2"})

    def test_a_panel_is_disarmed_before_it_is_killed(self):
        """Otherwise the change undoes itself: `kill-pane` fires that pane's own
        `pane-died` hook, `cmd_respawn` waits out its backoff, finds the session still
        perfectly alive — only the layout changed — and puts the panel the operator just
        dismissed straight back, one respawn life poorer."""
        state.record_panes(self.fid, panels={"top": "%1", "right": "%3", "bottom": "%2"})
        _, fake = self._run("minimal")
        disarm = fake.where("set-hook", "-u", "%3")
        kill = fake.where("kill-pane", "%3")
        self.assertEqual(len(disarm), 1, fake.calls)
        self.assertEqual(len(kill), 1, fake.calls)
        self.assertLess(disarm[0], kill[0],
                        "the hook must be gone before the pane is")

    def test_the_resize_hook_names_the_frame_and_never_a_pane_id(self):
        """#488 turned the `window-resized` action from literal `resize-pane` text into a
        `run-shell` that calls charter back, because `bottom`'s height depends on the
        window and a constant is destructive once the window shrinks. This pins the
        consequence for #475 as well: NO pane id reaches the action text any more, so a
        pane id read back off disk cannot be interpolated into a command line tmux
        re-parses — not the killed one, and not the kept ones either."""
        state.record_panes(self.fid, panels={"top": "%1", "right": "%3", "bottom": "%2"})
        _, fake = self._run("minimal")
        hooks = [c for c in fake.calls if "window-resized" in c]
        self.assertEqual(len(hooks), 1, fake.calls)
        action = hooks[0][-1]
        self.assertIn("frame-resize", action)
        self.assertIn(self.fid, action)
        for pane in ("%1", "%2", "%3"):
            self.assertNotIn(pane, action, action)

    def test_dropping_every_slot_removes_the_resize_hook(self):
        """Reachable, not hypothetical: `_drawable_slots` answers `[]` below half of
        `min_cols`/`min_rows`, so shrinking a small window's frame kills every pane. The
        launch's own hook names those panes, and one `set-hook` only REPLACES a hook when
        there is something to replace it with — an empty map replaces nothing. Left as an
        early return, the hook survives firing `resize-pane -t %1` at dead panes on every
        resize for the life of the window."""
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2",
                                             "repos": "%5", "right": "%4"})
        rc, fake = self._run("minimal", fake=_Tmux(size="40:8"))
        self.assertEqual(rc, 0)
        self.assertEqual(len([c for c in fake.calls if "kill-pane" in c]), 4, fake.calls)
        unset = [c for c in fake.calls if "window-resized" in c and "-u" in c]
        self.assertEqual(len(unset), 1, fake.calls)
        self.assertEqual(state.panes(self.fid), {})

    def test_surviving_panels_have_their_size_re_asserted(self):
        """tmux redistributes every remaining pane proportionally on a `kill-pane`, so
        the panels that merely SURVIVED a density change are exactly the ones a `-l` on a
        newly split pane cannot fix."""
        _, fake = self._run("full")
        resized = {c[c.index("-t") + 1] for c in fake.calls if "resize-pane" in c}
        self.assertIn("%1", resized)
        self.assertIn("%2", resized)

    def test_a_kept_slots_pane_id_off_disk_is_shape_checked_before_it_is_carried(self):
        """#475, and the guard it is actually about. `state.panes` is a file under the
        frame's own directory, so whoever can write there decides what these strings say
        — and `_relayout`'s shape check used to sit BELOW the `want` branch, guarding
        only the slot being killed. Every slot the new density KEPT went into `keep`
        unexamined, and `keep` is what gets resized, re-recorded, and (before #488) named
        in a hook action.

        **Asserted on the RECORDED MAP, not on the tmux argv, and that is what makes it
        falsifiable on its own.** `_reassert_sizes` checks every id it is handed too, so
        an argv assertion passes with this guard deleted — a guard passing because a
        DIFFERENT guard caught it. Nothing downstream re-writes `state.panes`, so what
        lands there can only have come from this branch: a refused id means the slot is
        absent from `keep`, is re-split fresh (`%7`, from the fake), and the bad string
        never reaches disk to be read again by the next keypress."""
        state.record_panes(self.fid,
                           panels={"top": "%1", "repos": "%5",
                                   "bottom": "%2;kill-server"})
        self._run("normal")
        recorded = state.panes(self.fid)
        self.assertEqual(recorded, {"top": "%1", "repos": "%5", "bottom": "%7"},
                         recorded)

    def test_reassert_sizes_refuses_a_pane_id_of_the_wrong_shape_itself(self):
        """The second half of the same rule, pinned where the builder lives rather than
        where its callers do. `_panel_died_hook_argv`'s own docstring states it — "every
        value that reaches the text is what decides, never where it came from" — and #475
        was exactly a helper documented as "safe because my caller checked" growing a
        second caller. `cmd_resize` IS that second caller: it reads `state.panes` off
        disk and hands it straight here, with no `_relayout` in between.

        Called directly, with a hostile id its callers would have filtered, so the
        assertion cannot be satisfied by somebody else's guard. The HARNESS pane is
        hostile here too, and for the same reason: `cmd_resize` reads that one off disk as
        well, and #515 gave this function a `resize-pane -t <harness>` of its own."""
        calls = []
        with mock.patch("charter.frame.tmuxctl.run",
                        side_effect=lambda a, argv, **k: calls.append(list(argv))):
            commands_frame._reassert_sizes(
                "charter", fid=self.fid,
                panes={"top": "%1", "bottom": "%2;kill-server"},
                harness_pane="%0;kill-server",
                window_cols=200, window_rows=50)
        targets = [c[c.index("-t") + 1] for c in calls if "resize-pane" in c]
        self.assertEqual(targets, ["%1"], calls)

    def test_the_harness_pane_gets_focus_back(self):
        """`split-window` makes each new pane ACTIVE — without this the operator is left
        typing into a panel."""
        _, fake = self._run("full")
        self.assertTrue(fake.where("select-pane", "%0"), fake.calls)

    def test_the_density_is_recorded_before_the_panes_move(self):
        """`cmd_density`'s own docstring claims this ordering and nothing tested it. It is
        the difference between a re-layout that dies halfway leaving the surviving panels
        drawing at the density the operator asked for, and one leaving them at the old one
        with no way to tell which. Asserted by reading the recorded value from INSIDE the
        re-layout, which is the only moment the order is observable."""
        seen = []
        real = commands_frame._relayout
        with mock.patch("charter.commands_frame._relayout",
                        side_effect=lambda *a, **k: (seen.append(state.density(self.fid)),
                                                     real(*a, **k))[1]):
            self._run("minimal")
        self.assertEqual(seen, ["minimal"],
                         "the density was recorded after the panes moved")

    def test_the_version_is_bumped_so_surviving_panels_repaint(self):
        before = state.version(self.fid)
        self._run("minimal")
        self.assertNotEqual(state.version(self.fid), before)

    def test_the_palettes_mark_moves_to_the_level_now_in_effect(self):
        """Nothing is re-recorded for this to be true: `_current_density` is read when the
        palette opens, so the mark follows the frame's own record by construction."""
        self._run("full")
        reg = builtin_actions.build(self.fid,
                                    current_density=commands_frame._current_density(self.fid))
        titles = [a.title for a in reg.all() if a.id.startswith("density.")]
        on = builtin_actions.MARK[0]
        self.assertIn(f"{on}density: full", titles)
        self.assertEqual([t for t in titles if t.startswith(on)],
                         [f"{on}density: full"], titles)

    def test_a_terminal_too_small_for_the_level_still_drops_the_side_panel(self):
        """A density change goes through the SAME size floors a launch does — asking for
        `full` in an 80-column terminal must not split a pane the frame has no room for.
        `_drawable_slots` is what enforces it, and it is asked here rather than
        reimplemented."""
        rc, fake = self._run("full", fake=_Tmux(size="80:20"))
        self.assertEqual(rc, 0)
        self.assertFalse([c for c in fake.calls if "split-window" in c], fake.calls)

    def _split_env(self, calls):
        """``{NAME: value}`` from every `-e` on the `split-window` calls in *calls*."""
        out = {}
        for c in calls:
            if "split-window" not in c:
                continue
            for i, x in enumerate(c):
                if x == "-e" and i + 1 < len(c):
                    name, _, value = c[i + 1].partition("=")
                    out[name] = value
        return out

    def test_a_new_pane_carries_the_frame_identity_on_either_server(self):
        """#411, one command over. A panel resolves its plane and its frame from its own
        environment, and a pane created by `split-window` gets that environment from the
        server — which on charter's shared private socket may have been started by ANOTHER
        launcher days ago, and on the operator's certainly was. `cmd_launch` states the
        identity explicitly on both paths since #412; a re-layout creates panes the same
        way and had to be taught the same thing rather than inheriting the rule from where
        the code happened to sit.

        Asserted against `commands_frame._FRAME_IDENTITY` itself, never a list of my own:
        that tuple has already grown from four names to five, and a test carrying a
        parallel copy would go on passing while the fifth never reached a re-laid-out
        pane."""
        for server in (None, "/private/tmp/tmux-502/default"):
            with self.subTest(server=server or "charter's own"):
                # Reset the map each time: the previous iteration left the frame AT
                # `full`, so without this the second subtest splits nothing and its
                # assertions are vacuously true — the exact shape of unfailable test this
                # suite has been bitten by.
                state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
                if server:
                    state.record_server(self.fid, server)
                _, fake = self._run("full")
                carried = self._split_env(fake.calls)
                self.assertTrue([c for c in fake.calls if "split-window" in c])
                for name in commands_frame._FRAME_IDENTITY:
                    self.assertIn(name, carried, carried)
                self.assertEqual(carried["CHARTER_SESSION_ID"], self.fid)

    def test_a_new_pane_gets_the_FRAMES_identity_not_this_processes(self):
        """#411, arriving on the one command charter has added since.

        `cmd_density` normally runs as a `subprocess.run` child of `cmd_action`, itself a
        `run-shell` child of the tmux server — and charter's private server is SHARED, so
        that child reads whichever launcher's environment started it. Only
        `CHARTER_SESSION_ID` is session-scoped (charter issues four `set-environment`
        calls and none covers the other four names). Measured on tmux 3.7c, a `run-shell`
        on the second frame reported the FIRST frame's workspace and harness beside its
        own id.

        So building the `-e` payload from `os.environ` pins another frame's plane onto the
        panes this keypress creates — and `$CHARTER_ROOT` wins outright in
        `root.find_root`, `$CHARTER_WORKSPACE` in `workspace.resolve`, so the new panels
        would draw a different plane from the ones that survived.

        The environment here is deliberately WRONG in every name the launcher recorded:
        the test fails unless the values come from `state.identity`. The previous test of
        this ran in-process with a correct `os.environ` and could not tell the two
        apart."""
        state.record_identity(self.fid, {"CHARTER_SESSION_ID": self.fid,
                                         "CHARTER_ROOT": "/planes/mine",
                                         "CHARTER_WORKSPACE": "my-ws",
                                         "CHARTER_HARNESS": "codex",
                                         "CHARTER_PERSONA": "steward"})
        someone_else = {"CHARTER_ROOT": "/planes/THEIRS",
                        "CHARTER_WORKSPACE": "THEIR-ws",
                        "CHARTER_HARNESS": "claude-code",
                        "CHARTER_PERSONA": "release"}
        with mock.patch.dict(os.environ, someone_else):
            _, fake = self._run("full")
        carried = self._split_env(fake.calls)
        self.assertEqual(carried["CHARTER_ROOT"], "/planes/mine", carried)
        self.assertEqual(carried["CHARTER_WORKSPACE"], "my-ws", carried)
        self.assertEqual(carried["CHARTER_HARNESS"], "codex", carried)
        self.assertEqual(carried["CHARTER_PERSONA"], "steward", carried)
        self.assertEqual(carried["CHARTER_SESSION_ID"], self.fid, carried)

    def test_an_unrecorded_identity_is_stated_empty_rather_than_inherited(self):
        """A frame launched by a charter that predates `record_identity`. Omitting the
        four unknown names would let the new pane inherit the SERVER's — which is the bug
        — so they are stated empty instead, which every charter reader already treats as
        absent (`workspace.resolve` and `root.find_root` test for truth, not presence).
        The pane then resolves from its own cwd, inherited from the harness pane, which is
        right. `_frame_identity_env` makes the identical argument for the launch path."""
        (state.frame_dir(self.fid, create=True) / "identity").unlink(missing_ok=True)
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "THEIR-ws"}):
            _, fake = self._run("full")
        carried = self._split_env(fake.calls)
        self.assertEqual(sorted(carried), sorted(commands_frame._FRAME_IDENTITY), carried)
        self.assertEqual(carried["CHARTER_WORKSPACE"], "", carried)
        self.assertEqual(carried["CHARTER_SESSION_ID"], self.fid,
                         "the one name this process can always be sure of")

    def test_a_launch_records_the_identity_a_later_keypress_will_read(self):
        """The other end of the same mechanism: nothing reads `state.identity` correctly
        if no launch ever writes it."""
        fake = _Tmux(new_panes=["%11", "%12"])
        env = {"CHARTER_ROOT": "/planes/mine", "CHARTER_WORKSPACE": "my-ws"}
        with mock.patch.dict(os.environ, env):
            commands_frame.state.record_identity(
                self.fid, commands_frame._frame_identity_env(
                    commands_frame._frame_env(self.fid, None)))
        recorded = state.identity(self.fid)
        self.assertEqual(recorded["CHARTER_ROOT"], "/planes/mine", recorded)
        self.assertEqual(recorded["CHARTER_SESSION_ID"], self.fid, recorded)
        self.assertEqual(sorted(recorded), sorted(commands_frame._FRAME_IDENTITY))

    def test_a_new_pane_carries_nothing_beyond_that_identity(self):
        """**A tmux `-e` is argv, and argv is not private** — world-readable in
        `/proc/<pid>/cmdline`, visible to `ps` for every local user, recorded by exec
        audit. Measured on a real environment, the full `_frame_env` came to 138 argv
        elements and 7,696 bytes carrying two live service-account tokens.

        `_launch_in_operator_tmux` passed the whole environment there until #446 closed
        it; `layout._env_argv` now refuses an unlisted name outright, so this cannot come
        back at any call site. Pinned here as well with a sentinel that could only arrive
        by the whole environment being handed over — the funnel's own guard is tested in
        `tests/test_frame_layout.NothingUnnamedReachesACommandLine`, and this is the
        keypress path actually reaching it.

        This is also what covers the launching pane's own identity: `TMUX`, `TMUX_PANE`,
        `COLUMNS` and `LINES` describe the pane `cmd_density` was FIRED in, and an equality
        against `_FRAME_IDENTITY` excludes them by construction. A separate test asserting
        just those four was written first and deleted — it could not fail while this one
        passed, which makes it a claim rather than a check."""
        state.record_server(self.fid, "/private/tmp/tmux-502/default")
        with mock.patch.dict(os.environ, {"AWS_SECRET_ACCESS_KEY": "SENTINEL-0xC0FFEE"}):
            _, fake = self._run("full")
        carried = self._split_env(fake.calls)
        self.assertEqual(sorted(carried), sorted(commands_frame._FRAME_IDENTITY), carried)
        self.assertNotIn("SENTINEL-0xC0FFEE", " ".join(x for c in fake.calls for x in c))

    def test_a_tmux_too_old_for_pane_env_is_given_the_pane_anyway(self):
        """Below `tmuxctl.PANE_ENV_FLOOR`, `split-window` cannot parse `-e` at all — and a
        flag tmux refuses takes the whole command with it. A pane created without its
        identity is degraded; a pane not created is a hole in the frame."""
        self.enterContext(mock.patch("charter.frame.tmuxctl.version", return_value=(2, 9)))
        _, fake = self._run("full")
        splits = [c for c in fake.calls if "split-window" in c]
        self.assertTrue(splits, fake.calls)
        for c in splits:
            self.assertNotIn("-e", c, c)

    def test_detaching_is_offered_with_its_reason_inside_an_operators_tmux(self):
        """There is no table to record and none to refuse to record any more — the palette
        is built when it opens. What used to be "no menu is written on the operator's own
        server" is now a ROW that says why: `detach-client -s <fid>` names a SESSION, and
        inside an operator's tmux a frame is a WINDOW, so the row is offered with the
        operator's own prefix key named in place of the thing charter cannot do."""
        state.record_server(self.fid, "/private/tmp/tmux-502/default")
        self._run("full")
        reg = builtin_actions.build(self.fid, current_density="full")
        offer = [o for o in reg.offers(fid=self.fid, snapshot={})
                 if o.id == "frame.detach"][0]
        self.assertFalse(offer.available)
        self.assertIn("your own prefix key", offer.reason)

    def test_detaching_is_available_on_charters_own_server(self):
        """The other direction, so the row above cannot pass by never being available."""
        self._run("full")
        reg = builtin_actions.build(self.fid, current_density="full")
        offer = [o for o in reg.offers(fid=self.fid, snapshot={})
                 if o.id == "frame.detach"][0]
        self.assertTrue(offer.available)
        self.assertEqual(offer.reason, "")

    def test_a_new_pane_inside_an_operators_tmux_is_armed_against_that_server(self):
        """#408. This used to assert the opposite — `_arm_panel_respawn` refused on the
        operator's server, because `cmd_respawn` and `_panel_died_hook_argv` both spelled
        `-L charter` by hand and would have aimed the hook at charter's private server.
        Both build their argv through `tmuxctl.server_argv` now, so the pane IS armed and
        the hook names the operator's socket by PATH.

        The `-S` is what this asserts, not merely that some hook was installed: a hook
        armed with `-L /private/tmp/…` would satisfy "a pane-died command was issued" and
        still be the whole defect."""
        state.record_server(self.fid, "/private/tmp/tmux-502/default")
        _, fake = self._run("full")
        armed = [c for c in fake.calls if "pane-died" in c and "-u" not in c]
        self.assertEqual(len(armed), 1, fake.calls)
        for cmd in armed:
            self.assertEqual(cmd[:3],
                             ["tmux", "-S", "/private/tmp/tmux-502/default"])
            self.assertIn(f"--frame {self.fid}", cmd[-1])

    def test_a_new_pane_on_charters_own_server_is_armed_for_respawn(self):
        """The other direction, so the refusal above cannot be satisfied by never arming
        anything anywhere."""
        _, fake = self._run("full")
        armed = [c for c in fake.calls if "pane-died" in c and "-u" not in c]
        self.assertEqual(len(armed), 1, fake.calls)

    def test_no_session_id_is_a_quiet_no_op(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": ""}):
            rc, fake = self._run("full")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [])
        self.assertIsNone(state.density(self.fid))

    def test_an_unknown_level_is_refused_before_anything_moves(self):
        """The level reaches a slot list and a palette row; the closed set is the guard,
        and it must be asked BEFORE the frame is touched, not after."""
        rc, fake = self._run("enormous")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [])
        self.assertIsNone(state.density(self.fid))

    def test_a_frame_with_no_recorded_harness_pane_is_a_quiet_no_op(self):
        """A frame launched by a charter that predates `record_harness_pane`. Every split
        carves off that one pane, so without it there is nothing to split from — and
        guessing at a pane id is the trap `frame/layout.py` measures."""
        (state.frame_dir(self.fid, create=True) / "harness").unlink()
        rc, fake = self._run("full")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [])

    def test_an_empty_panel_map_is_not_a_refusal(self):
        """The other half, and a real case rather than a hypothetical: a frame whose
        panels all failed to draw (`split-window` reporting no id) records an empty map,
        and it can still be given panels. Refusing on the panel map would leave the one
        frame that most needs a re-layout unable to have one — and it is exactly the
        shape a guard written as `if not panels: return 0` would have produced."""
        state.record_panes(self.fid, panels={})
        rc, fake = self._run("full")
        self.assertEqual(rc, 0)
        split_slots = sorted(c[c.index("panel") + 1] for c in fake.calls
                             if "split-window" in c and "panel" in c)
        self.assertEqual(split_slots, ["bottom", "repos", "right", "top"], fake.calls)

    def test_a_harness_pane_that_is_not_tmuxs_own_shape_is_refused(self):
        """The id comes back off disk, not off `split-window`'s stdout, so it gets the
        same `_PANE_ID_RE` treatment on the way out that it got on the way in — it is
        about to be interpolated into a hook action tmux re-parses."""
        state.record_harness_pane(self.fid, "%0; kill-server")
        rc, fake = self._run("full")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [])


class ResizeRecomputesForBothDimensions(PersonaIso, unittest.TestCase):
    """#500: `charter frame-resize` re-applies a height that matches what the panel will
    draw in the window it just measured — at that window's WIDTH and this frame's
    DENSITY, not from the repo count alone.

    This is the surface the operator actually hits, and it is not launch-only: the
    `window-resized` hook fires on every step of a terminal drag, so a table pane sized
    for a table it can no longer draw is re-asserted continuously for as long as the
    terminal stays narrow — with the harness pinned at `layout.HARNESS_MIN_ROWS` the
    whole time (measured on tmux 3.7c: a 26-row, 80-column window with 14 repos gave that
    pane 11 rows to draw one line in).

    Driven through `cmd_resize` itself rather than through `_reassert_sizes`, because
    what shipped broken was the call site: it measured the width into `_cols` and threw
    it away.
    """

    def setUp(self):
        super().setUp()
        self.fid = f"rsz-{_a_dead_pid()}"
        state.record_harness_pane(self.fid, "%0")
        # Every slot this plane's arrangement names, so a resize at the sizes below finds
        # the frame ALREADY the shape it should be and re-sizes rather than re-lays-out.
        # #536 made that distinction matter: a map missing `right` is a frame that would
        # gain a sidebar on its next resize, which is correct behaviour and is not what
        # this class is about.
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2",
                                             "repos": "%5", "right": "%3"})
        # `_relayout_target` asks the real binary otherwise, so a machine with no tmux (or
        # one too old for `window-resized`) would take a different branch here than CI —
        # #519's shape exactly. `LiveOverride` above pins its own for the same reason.
        self.enterContext(mock.patch("charter.frame.tmuxctl.version",
                                     return_value=(3, 7)))
        # The settle wait is pinned by `TheShapeWaitsForTheWindowToStop` below and would
        # otherwise cost this class a real 400ms per shape change.
        self.enterContext(mock.patch.object(commands_frame, "_SETTLE_SECONDS", 0.0))
        rows = [{"name": f"repo{i}", "branch": "main", "dirty": False,
                 "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None,
                 "change": None, "sigil": "", "current": False, "worktree_count": 0}
                for i in range(6)]
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": rows, "worktrees": []})

    def _heights(self, size, *, panels=None) -> dict[str, int]:
        """Every `resize-pane -y` `cmd_resize` issues, as ``{pane id: rows}``.

        **The table pane is deliberately absent from this map, and that is the property**
        (#515). tmux's `resize-pane -y` moves one boundary, so in a stack of N panes only
        N-1 heights are free; the one left unasserted is `repos`, whose height is already
        a function of the others, and the HARNESS (`%0`) is asserted instead. So what
        `cmd_resize` decided about the table is read here as the harness's height — and
        `tests/test_frame_tmux_integration.py` is where a real tmux confirms the table
        lands on the rows that leaves it.
        """
        if panels is not None:
            state.record_panes(self.fid, panels=panels)
        fake = _Tmux(size=size)
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            rc = commands_frame.cmd_resize(SimpleNamespace(frame=self.fid))
        self.assertEqual(rc, 0)
        return {c[c.index("-t") + 1]: int(c[c.index("-y") + 1])
                for c in fake.calls if "resize-pane" in c and "-y" in c}

    def _table_rows(self, size, *, panels=None) -> int:
        """How many rows `cmd_resize` left the table pane, read back out of the window.

        Not an independent re-derivation: the window's height minus every height
        `cmd_resize` actually asserted, minus one border row per horizontal pane. That is
        the same subtraction tmux itself performs when it honours those `-y`s, which is
        why `test_frame_tmux_integration` can check this number against a real
        `#{pane_height}` rather than against another copy of charter's arithmetic.
        """
        heights = self._heights(size, panels=panels)
        window_rows = int(size.split(":")[1])
        # One border row per SPLIT, and the splits are one fewer than the panes in the
        # stack — which is `len(heights)` named panes plus the table itself.
        return window_rows - sum(heights.values()) - len(heights)

    def test_a_wide_window_keeps_the_table_sized_pane(self):
        """The control. Without it the narrow assertion below would pass against a
        function that always answered one."""
        self.assertEqual(self._table_rows("200:50"), 1 + 6)

    def test_the_fixed_strips_are_re_asserted_at_their_own_constant_height(self):
        """The rest of the map, and the check that #515's arithmetic reached this call
        site. `bottom` went back to being one row; a `_reassert_sizes` still treating it
        as the variable slot would hand it the table's height and take those rows out of
        the harness on every step of a drag. The harness's own `-y` is asserted here too,
        because before #515 nothing named it at all — with three strips instead of two,
        the two below the harness trade rows with each other and never with it."""
        self.assertEqual(self._heights("200:50"), {"%1": 1, "%2": 1, "%0": 38})

    def test_a_table_pane_that_stays_is_shrunk_to_the_one_row_it_can_draw(self):
        """The panel draws no table below `statusline._LEFT_W`, so every row past the
        first was blank — and came out of the harness.

        **Driven through `_reassert_sizes` rather than `cmd_resize` since #536**, and the
        change of call site is the fix rather than a convenience: a `cmd_resize` at these
        widths now removes the pane outright (the test below), so the one-row height is
        what the SIZING half answers for a table pane that is still there — a re-layout
        that has not settled yet, a frame whose kill failed, a `[frame] slots` a plane
        committed by hand. That arithmetic is #500's and it must not rot just because the
        commonest route to it changed.

        Read back the way `_table_rows` reads it: the window's rows minus every height
        asserted, minus one border per horizontal pane.
        """
        for cols in (60, 80, statusline._LEFT_W - 1):
            with self.subTest(cols=cols):
                fake = _Tmux(size=f"{cols}:50")
                panes = {"top": "%1", "bottom": "%2", "repos": "%5"}
                with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
                    commands_frame._reassert_sizes(
                        "sock", fid=self.fid, panes=panes, harness_pane="%0",
                        window_cols=cols, window_rows=50)
                heights = {c[c.index("-t") + 1]: int(c[c.index("-y") + 1])
                           for c in fake.calls if "resize-pane" in c and "-y" in c}
                self.assertEqual(50 - sum(heights.values()) - len(heights), 1)

    def test_the_density_the_operator_chose_is_read_here_too(self):
        """`cmd_density` and `cmd_resize` are different processes minutes apart, so the
        level has to come off the frame's own state directory on this path rather than
        being remembered. A `minimal` frame that then gets resized must not be handed the
        `normal` height back."""
        state.record_density(self.fid, "minimal")
        self.assertEqual(self._table_rows("200:50"), 1 + slots._TERSE_ROWS)
        state.record_density(self.fid, "normal")
        self.assertEqual(self._table_rows("200:50"), 1 + 6)

    def test_a_pane_inset_beside_the_sidebar_is_resized_for_its_own_width(self):
        """Round 3. The window's width is not the PANE's when the frame's `[frame] slots`
        put `right` before `repos`: `panel_argvs` splits both off the harness pane in
        list order, so the table comes off a harness that is already 23 columns narrower —
        measured on tmux 3.7c, 87 columns in a 110-column window, which is below
        `statusline._LEFT_W` and draws no table at all.

        `cmd_resize` never consults a slot list, so this was the longer-lived half of the
        defect: it re-applied the over-tall pane on every step of a terminal drag. What it
        does have is the recorded pane map, whose insertion order IS the order those panes
        were split in — `_split_panels` writes it that way and JSON round-trips it — so
        `layout.repos_cols` can turn the window's width into the pane's.

        The control is the same window and the same map with the shipped order, so a fix
        that stopped sizing the table for its content at all is red rather than green.
        """
        self.assertEqual(
            self._table_rows("110:50",
                             panels={"right": "%3", "top": "%1", "bottom": "%2",
                                     "repos": "%5"}),
            1)
        self.assertEqual(
            self._table_rows("110:50",
                             panels={"top": "%1", "bottom": "%2", "repos": "%5",
                                     "right": "%3"}),
            1 + 6)


class _ResizeFixture(PersonaIso, unittest.TestCase):
    """One running frame at four slots, and one way to drive `charter frame-resize` at it.

    Shared by the three classes below because #591, #510, #501 and #536 are one property
    said at four call sites — *a pane's geometry comes from the pane, and a decision made
    from a stale reading is refused rather than applied* — and a second fixture would be a
    second frame for them to disagree about.
    """

    #: Every slot this plane's arrangement names, in split order, already drawn. A resize
    #: at a size that keeps all four therefore finds the frame the shape it should be.
    PANES = {"top": "%1", "bottom": "%2", "repos": "%5", "right": "%3"}

    def setUp(self):
        super().setUp()
        self.fid = f"rsz-{_a_dead_pid()}"
        state.record_harness_pane(self.fid, "%0")
        state.record_panes(self.fid, panels=dict(self.PANES))
        self.enterContext(mock.patch("charter.frame.tmuxctl.version",
                                     return_value=(3, 7)))
        self.enterContext(mock.patch.object(commands_frame, "_SETTLE_SECONDS", 0.0))
        rows = [{"name": f"repo{i}", "branch": "main", "dirty": False,
                 "tracked_dirty": False, "ahead": 0, "behind": 0, "ci": None,
                 "change": None, "sigil": "", "current": False, "worktree_count": 0}
                for i in range(6)]
        gather.save(self.fid, {"gathered_at": 0.0, "workspace": "w",
                               "current_repo": None, "repos": rows, "worktrees": []})

    def _resize(self, **kwargs) -> _Tmux:
        """Fire one `charter frame-resize` child and hand back what it said to tmux."""
        fake = _Tmux(**kwargs)
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            self.assertEqual(commands_frame.cmd_resize(SimpleNamespace(frame=self.fid)), 0)
        return fake

    @staticmethod
    def _resized(fake: _Tmux) -> list[list[str]]:
        return [c for c in fake.calls if "resize-pane" in c]

    @staticmethod
    def _killed(fake: _Tmux) -> list[str]:
        return [c[c.index("-t") + 1] for c in fake.calls if "kill-pane" in c]

    @staticmethod
    def _split(fake: _Tmux) -> list[list[str]]:
        return [c for c in fake.calls if "split-window" in c]


class AStaleMeasurementIsRefusedRatherThanApplied(_ResizeFixture):
    """#501. `window-resized` fires per size change and each event starts its own
    backgrounded `charter frame-resize`; nothing serialises them and `run-shell -b` gives
    no completion ordering, so during a drag several children are in flight at once, each
    holding a measurement from a different instant.

    That is not cosmetic. `bottom`'s height is `min(content, cap)` where the cap is what
    the window can spare, so a child that measured a TALLER window computes a taller pane;
    landing after the child that measured the final, shorter one, it hands tmux a size for
    a window that no longer exists — and tmux does not refuse an over-large `-y`, it grants
    it out of the neighbour, which is the agent's own session (measured on 3.7c:
    `resize-pane -y 40` in a 20-row window left the harness pane **1 row tall**).

    Reachable by construction rather than by hand, exactly as #501 said: the fake below
    answers one window size to the measurement and a different one to the re-read, which is
    what a second child changing the window underneath this one looks like from in here.
    """

    def test_a_size_the_window_has_already_left_is_not_applied(self):
        """The measurement says 50 rows; by the time this child is ready to apply, the
        window is 22. Nothing may be resized — the change that beat this check fired its
        own `window-resized`, and that event's child measures the window as it now is."""
        fake = self._resize(sizes=["200:50", "200:22"])
        self.assertEqual(self._resized(fake), [],
                         "a child applied a size for a window it had already been told "
                         "was gone")

    def test_the_measurement_that_still_matches_is_applied(self):
        """The control, and it is what makes the assertion above mean anything: the same
        code path with a window that has NOT moved must still do its whole job, or
        "nothing was resized" would be satisfied by a `cmd_resize` that had simply stopped
        working."""
        fake = self._resize(sizes=["200:50", "200:50"])
        self.assertTrue(self._resized(fake),
                        "a stable window was refused too — the check is not a check, it "
                        "is an off switch")

    def test_a_window_tmux_will_not_report_is_not_guessed_at(self):
        """The refusal one step earlier. `_window_size` answers `_FALLBACK_SIZE` for a
        window tmux would not report, which is right for a LAUNCHER — it has to draw
        something — and wrong here: 80x24 asserted over a window that is very probably not
        80x24 is the same destructive move as a stale measurement, with less excuse.
        `_measure_window` says `None` and this child does nothing at all."""
        fake = self._resize(size="not-a-size")
        self.assertEqual(self._resized(fake), [])
        self.assertEqual(self._killed(fake), [])

    def test_the_re_read_happens_before_anything_is_applied_not_after(self):
        """The ordering the whole fix is. A re-read AFTER the `resize-pane` calls would
        satisfy every assertion above about the final state while still having applied the
        stale size first — and tmux takes an over-large height out of the harness the
        moment it is asked, not when charter finds out it was wrong."""
        fake = self._resize(sizes=["200:50", "200:50"])
        window_reads = [i for i, c in enumerate(fake.calls)
                        if commands_frame._WINDOW_SIZE_FORMAT in c]
        first_apply = min(i for i, c in enumerate(fake.calls) if "resize-pane" in c)
        self.assertEqual(len(window_reads), 2,
                         f"the window was read {len(window_reads)} times, not measured "
                         f"and then re-read: {fake.calls}")
        self.assertLess(window_reads[1], first_apply,
                        "the re-read landed after the sizes had already been applied")


class TheTablePaneIsAskedHowWideItIs(_ResizeFixture):
    """#510. `layout.repos_cols` turns the WINDOW's width into the PANE's by walking the
    order those panes were split in — correct, and a derivation where a measurement is
    available: the pane's id is in the recorded map and tmux will answer for it.

    The two part company silently. The order comes off disk as JSON, and `state.panes`
    validates the VALUES and says nothing about the order, so a truncated write or a hand
    edit reaches here as a plausible map whose order is fiction; the failure is an
    over-tall pane re-asserted on every step of a drag with the rows taken off the harness.
    """

    def test_the_panes_own_width_is_asked_of_tmux_and_beats_the_derivation(self):
        """The map says the shipped order, in which nothing insets the table — so the
        derivation says 200 and sizes the pane for all six repos. tmux says the pane is
        actually 60 columns, which is below `statusline._LEFT_W` and draws no table at
        all. The measurement wins, and the harness gets the five rows back."""
        fake = self._resize(size="200:50", pane_cols=60)
        heights = {c[c.index("-t") + 1]: int(c[c.index("-y") + 1])
                   for c in self._resized(fake) if "-y" in c}
        self.assertEqual(50 - sum(heights.values()) - len(heights), 1,
                         f"the table pane was sized from the derivation, not from the "
                         f"width tmux reported for it: {heights}")

    def test_a_pane_tmux_will_not_measure_falls_back_to_the_derivation(self):
        """The launcher cannot measure a pane that does not exist yet, so `repos_cols`
        stays charter's answer and this is the running frame's version of the same case: a
        pane that has died between the map being read and this running. Same window, same
        map, no answer from tmux — the six-repo table comes back."""
        fake = self._resize(size="200:50", pane_cols=None)
        heights = {c[c.index("-t") + 1]: int(c[c.index("-y") + 1])
                   for c in self._resized(fake) if "-y" in c}
        self.assertEqual(50 - sum(heights.values()) - len(heights), 1 + 6)

    def test_a_zero_is_not_an_answer(self):
        """`repos_cols` floors at 0 and a pane that has gone away can report one. Telling
        `repos_rows_wanted` the table has no room at all would floor the pane at one row
        for a reason that is a read failure rather than a geometry."""
        fake = self._resize(size="200:50", pane_cols=0)
        heights = {c[c.index("-t") + 1]: int(c[c.index("-y") + 1])
                   for c in self._resized(fake) if "-y" in c}
        self.assertEqual(50 - sum(heights.values()) - len(heights), 1 + 6)

    def test_the_side_panels_width_lands_before_the_pane_is_measured(self):
        """**The order is the whole of why the measurement can be trusted, and it was
        measured rather than reasoned.** tmux redistributes every pane proportionally on a
        window resize, so a sidebar mid-drag is not 22 columns wide and the pane beside it
        is not the width it is about to be: on tmux 3.7c a 120x40 frame with `right` split
        first, grown to 200x40, came back with `right` at **62** columns and the table pane
        reading **137** — where the truth, one `resize-pane -x 22` later, is **177**.
        Measuring first would have been worse than deriving. So the columns are applied,
        and only then is the pane asked."""
        fake = self._resize(size="200:50", pane_cols=177)
        first_x = min(i for i, c in enumerate(fake.calls)
                      if "resize-pane" in c and "-x" in c)
        measured = min(i for i, c in enumerate(fake.calls)
                       if commands_frame._PANE_WIDTH_FORMAT in c)
        self.assertLess(first_x, measured,
                        "the table pane was measured while the sidebar was still "
                        "wherever tmux's proportional redistribution had left it")

    def test_the_rows_land_after_the_measurement(self):
        """The other half of the same order: a height computed from the measured width is
        worth nothing if it was asserted before the width was known."""
        fake = self._resize(size="200:50", pane_cols=60)
        measured = min(i for i, c in enumerate(fake.calls)
                       if commands_frame._PANE_WIDTH_FORMAT in c)
        first_y = min(i for i, c in enumerate(fake.calls)
                      if "resize-pane" in c and "-y" in c)
        self.assertLess(measured, first_y)


class TheMeasurementsOwnRefusals(PersonaIso, unittest.TestCase):
    """`_variable_pane_cols`, asked directly rather than through `cmd_resize`.

    `tools/sweep.py` found two survivors sitting inside this one function and said the
    thing that matters about them: two guards in sequence mask each other, so neither is
    safe to call equivalent on its own. Both were reachable only through `cmd_resize`,
    where a later filter happened to catch what an earlier one let past. Each is asked its
    own question here.
    """

    _PANES = {"top": "%1", "bottom": "%2", "repos": "%5"}

    def _cols(self, *, returncode=0, stdout="", panes=None) -> tuple[int, list[list[str]]]:
        calls: list[list[str]] = []

        def fake(action, argv, *, env=None, timeout=None, report=True):
            calls.append(list(argv))
            return subprocess.CompletedProcess(argv, returncode, stdout=stdout, stderr="")

        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            got = commands_frame._variable_pane_cols(
                "sock", panes=dict(self._PANES if panes is None else panes),
                window_cols=200)
        return got, calls

    def test_tmuxs_answer_is_the_answer(self):
        """The control. Without it every refusal below is satisfied by a function that
        never believes tmux at all."""
        self.assertEqual(self._cols(stdout="87\n")[0], 87)

    def test_a_failure_whose_stdout_still_parses_is_not_an_answer(self):
        """The `out.returncode == 0` half, which `cols.isdigit()` looks like it already
        covers and does not. `tmuxctl.run` folds a TIMEOUT into a return code
        (`tmuxctl.TIMED_OUT`) and hands back whatever the killed process had already
        written — so a partial read is a failure whose stdout is a perfectly good number.
        Believing it sizes the table pane from a truncated measurement."""
        self.assertEqual(self._cols(returncode=1, stdout="6")[0], 200,
                         "a failed `display-message` was believed because its stdout "
                         "happened to parse")

    def test_a_zero_is_not_an_answer(self):
        self.assertEqual(self._cols(stdout="0")[0], 200)

    def test_nothing_at_all_is_not_an_answer(self):
        self.assertEqual(self._cols(stdout="")[0], 200)

    def test_a_recorded_pane_id_that_is_not_tmuxs_own_shape_never_reaches_a_target(self):
        """#475's rule on the one value this path reads off disk. `state.panes` is JSON in
        the frame's own state directory and it validates that a value is a STRING, not
        that it is a pane — `%1;kill-server` is a string. The guard is asserted on what was
        ISSUED rather than on the number that came back, because the derivation is the
        answer either way: a version that skipped the check would return 200 too, having
        first handed that text to tmux."""
        got, calls = self._cols(stdout="87",
                                panes={"top": "%1", "repos": "%1;kill-server"})
        self.assertEqual(got, 200)
        self.assertEqual([c for c in calls if "display-message" in c], [],
                         f"a pane id off disk was used as a `-t` without ever having "
                         f"tmux's own shape: {calls}")

    def test_a_frame_with_no_variable_row_pane_asks_nothing(self):
        """A `[frame] slots` without `repos` — an ordinary frame, and the launcher's own
        case. There is nothing to measure, so nothing is measured and the derivation
        answers for a pane that would be split rather than one that is."""
        got, calls = self._cols(stdout="87", panes={"top": "%1", "bottom": "%2"})
        self.assertEqual(got, 200)
        self.assertEqual(calls, [])


class AResizeAddsAndRemovesPanesNotOnlySizesThem(_ResizeFixture):
    """#536. Which slots a frame HAD was decided once, at launch: `_drawable_slots` ran
    against the terminal the frame started in and only a density change re-ran it. So a
    frame launched at 200 columns and dragged to 80 kept a sidebar the same frame would
    have refused to draw had it started there, and a frame launched at 80 and widened to
    200 never gained one — the repo table's pane saying `⋯ too narrow for the repo table`
    is #515 being honest about a pane it could not un-split.
    """

    def test_narrowing_past_the_table_width_removes_the_pane_rather_than_shrinking_it(self):
        """`layout.visible_slots` drops `repos` below `statusline._LEFT_W` and `right`
        below `[frame] min-cols`, because a pane too narrow for the table is a bordered
        rectangle that reads as "this workspace has no repos" on a plane that has six. A
        LAUNCH at 80 columns already got that right; this is the drag getting it right."""
        fake = self._resize(size="80:50")
        self.assertEqual(sorted(self._killed(fake)), ["%3", "%5"],
                         f"the panes a launch at this size would not have drawn are still "
                         f"there: {fake.calls}")

    def test_widening_gains_the_pane_a_launch_at_that_size_would_have_had(self):
        """The direction that was strictly worse, because nothing at all brought it back:
        a frame launched narrow and then widened had to be relaunched, or sent through the
        F2 palette, to get the panes it now had room for."""
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
        fake = self._resize(size="200:50")
        split = [c for c in self._split(fake)]
        self.assertEqual(len(split), 2,
                         f"a widened frame did not gain the two panes it now has room "
                         f"for: {fake.calls}")

    def test_a_size_that_changes_nothing_moves_no_panes_at_all(self):
        """The control this whole class needs, and the property that keeps a drag cheap:
        every step of a drag that does not cross a boundary must be a re-SIZE and nothing
        else. A `cmd_resize` that killed and re-split at each step would spend a
        `charter panel` process, a cold charter import and one of three respawn lives per
        step."""
        fake = self._resize(size="200:50")
        self.assertEqual(self._killed(fake), [])
        self.assertEqual(self._split(fake), [])
        self.assertTrue(self._resized(fake))

    def test_the_last_pane_is_never_dropped(self):
        """Below half the size floors `layout.visible_slots` answers `[]`, and a
        `_relayout` with nothing left makes `_install_resize_hook` REMOVE the
        `window-resized` hook — correctly, and it is a one-way door reached from here: the
        hook is the only thing that would notice the terminal being widened again. So this
        frame keeps what it has at that size, exactly as it did before #536, and the
        operator who wants nothing drawn has a keypress that says so."""
        self.assertEqual(commands_frame._drawable_slots(
            40, 8, commands_frame._visible_now(self.fid, config.FRAME)), [],
            "this window is not below the floors — the case is not being exercised")
        fake = self._resize(size="40:8")
        self.assertEqual(self._killed(fake), [],
                         "a resize killed the last pane, and with it the hook that would "
                         "have brought any of them back")

    def test_a_panel_the_operator_hid_is_not_brought_back_by_a_resize(self):
        """The recompute starts from this frame's own arrangement minus its own hidden set
        (`_visible_now`), never from `config.FRAME["slots"]`. Recomputing from the config
        would put a panel the operator toggled off back on screen on their next terminal
        drag — a resize silently undoing a keypress."""
        state.record_hidden(self.fid, ["right", "repos"])
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
        fake = self._resize(size="200:50")
        self.assertEqual(self._split(fake), [],
                         f"a resize re-split panels the operator had hidden: {fake.calls}")

    def test_the_density_the_operator_chose_survives_a_resize_too(self):
        """The same property reached through the other key. A `minimal` frame is `top` and
        `bottom` and nothing else; a resize must not grow it back to `full` merely because
        the terminal is wide enough for one."""
        state.record_density(self.fid, "minimal")
        state.record_hidden(self.fid, [n for n in instance.frame_arrangement(config.FRAME)
                                       if n not in instance.density_slots("minimal")])
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
        fake = self._resize(size="200:50")
        self.assertEqual(self._split(fake), [])


class TheShapeWaitsForTheWindowToStop(_ResizeFixture):
    """#536's second half, and it is #501's mechanism used for a slower question.

    Re-applying a size out of order is self-correcting: the next event fixes it. Killing
    and re-splitting panes out of order is not — each split is a new interpreter, a cold
    charter import and a first paint, and it spends one of the three lives
    `_arm_panel_respawn` gives that pane. So a drag through the width where the table stops
    fitting must not thrash panes in and out at every step of it.

    There is no timer here and no state to debounce with: `notify._last` works because one
    process makes every call, and every one of these children is its own `run-shell -b`
    process. What they share is the window, so a child that wants to change the shape
    sleeps and then asks whether the window is still where it was. The one the drag
    actually ended on is the only one that gets a yes.
    """

    def setUp(self):
        super().setUp()
        # Undo the fixture's own zero — this class is what pins the wait, so it has to be
        # the real mechanism being exercised, only faster than 400ms.
        self.enterContext(mock.patch.object(commands_frame, "_SETTLE_SECONDS", 0.0))

    def test_a_window_still_moving_moves_no_panes(self):
        """Two sizes: the one this child measured, and the one the window is by the time
        the wait is over. Both are narrow enough to want the table pane gone, so a version
        that killed on the first measurement alone would still look right in the final
        state — hence the assertion is on what was ISSUED, not on what was wanted."""
        fake = self._resize(sizes=["80:50", "70:50"])
        self.assertEqual(self._killed(fake), [],
                         f"panes were killed for a window the drag had already left: "
                         f"{fake.calls}")

    def test_the_window_the_drag_ended_on_is_the_one_that_gets_acted_on(self):
        """The control. The same two-read path with a window that settled must do the
        whole job, or "nothing was killed" above is satisfied by a mechanism that never
        acts at all."""
        fake = self._resize(sizes=["80:50", "80:50"])
        self.assertEqual(sorted(self._killed(fake)), ["%3", "%5"])

    def test_the_layout_uses_the_measurement_that_settled_not_a_fresh_one(self):
        """The last place a third reading could sneak in, and `tools/sweep.py` found it:
        `_apply_arrangement` takes the window it is to lay out for, and collapsing that to
        its own `_window_size` call left every other test green.

        It is the same defect as #501 one layer up. This child measured 200x50, waited, and
        confirmed 200x50 — that is the window the decision was made from and the only one it
        may be applied to. A `_apply_arrangement` that measured again would get whatever the
        window happens to be at that instant, and act on a size nothing checked: three
        answers here, and the third is a terminal that has moved to 80 columns, where
        `_drawable_slots` wants two panes rather than four. Asserted on the SPLITS, because
        the two readings disagree about exactly which panes this frame should have.
        """
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
        fake = self._resize(sizes=["200:50", "200:50", "80:50"])
        self.assertEqual(len(self._split(fake)), 2,
                         f"the re-layout was laid out for a window nobody checked — the "
                         f"settled 200x50 measurement was thrown away: {fake.calls}")

    def test_the_wait_is_actually_waited(self):
        """`_SETTLE_SECONDS` is patched to zero everywhere above, which is exactly the
        shape of a settle that has been accidentally deleted. This asserts the sleep is
        reached — with the real constant's own value, and only on the path that changes
        the frame's shape."""
        slept: list[float] = []
        with mock.patch.object(commands_frame, "_SETTLE_SECONDS", 0.25), \
             mock.patch("charter.commands_frame.time.sleep", slept.append):
            self._resize(size="80:50")
        self.assertEqual(slept, [0.25])

    def test_a_resize_that_changes_no_panes_never_waits(self):
        """The cost bound. A drag that stays on one side of every boundary is the ordinary
        case and must pay nothing for a mechanism it does not use — otherwise every step
        of every drag leaves a sleeping interpreter behind for the length of the wait."""
        slept: list[float] = []
        with mock.patch("charter.commands_frame.time.sleep", slept.append):
            self._resize(size="200:50")
        self.assertEqual(slept, [])

    def test_a_frame_already_the_shape_its_operator_asked_for_never_waits_either(self):
        """The same bound, for the frame where "what this shows" and "what the config
        says" differ — which is every frame whose operator has pressed a key.

        Hand-mutating found this: computing `want` from `config.FRAME["slots"]` while
        `_apply_arrangement` still got `_visible_now`'s answer left every geometry
        assertion green, because the re-layout laid out the right thing. All it cost was a
        settle wait and a version bump on a resize with nothing to do — which is invisible
        in the final state and is exactly the "a second line hides the consequence" shape.
        A hidden `right` with the other three drawn is a frame that IS its own shape, so a
        resize must decide that in the same breath it decides what to lay out.
        """
        state.record_hidden(self.fid, ["right"])
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2", "repos": "%5"})
        slept: list[float] = []
        with mock.patch("charter.commands_frame.time.sleep", slept.append):
            fake = self._resize(size="200:50")
        self.assertEqual(slept, [],
                         "a frame already showing exactly what its operator asked for "
                         "waited out a settle to change nothing")
        self.assertEqual(self._split(fake), [])
        self.assertEqual(self._killed(fake), [])


class TheSettleIsLongEnoughToBeOne(unittest.TestCase):
    """`_SETTLE_SECONDS`' own VALUE, which every test that exercises the settle patches
    away and therefore cannot see: set it to `0.0` and `TheShapeWaitsForTheWindowToStop`
    above stays entirely green while the mechanism is gone. Found by hand-mutating the
    constant — `tools/sweep.py` has no operator for a number (#569).

    Deliberately a bare `TestCase` with no fixture at all: any class that drives
    `cmd_resize` has to patch this constant to stay fast, and a test reading a constant its
    own `setUp` replaced is asserting against the patch.
    """

    def test_the_wait_clears_one_measured_re_layout(self):
        """A re-layout's own tmux command list — two `set -p pane-died`, two
        `split-window`s, the `window-resized` hook, the window and pane measurements, four
        `resize-pane`s and a `select-pane` — was measured at a **median 72ms over 5 runs**
        on tmux 3.7c, and that is before either new panel's cold charter import and first
        paint. A wait under that lets one crossing of the boundary land inside the last
        one, which is the pane-thrash this mechanism exists to prevent."""
        self.assertGreater(commands_frame._SETTLE_SECONDS, 0.072,
                           "the settle is shorter than one measured re-layout — a second "
                           "crossing of the boundary lands inside the first")

    def test_the_wait_is_not_long_enough_to_read_as_a_missed_resize(self):
        """The ceiling, and it is the other failure rather than tidiness: the panes do not
        move until this elapses, so a settle an operator can sit through reads as the frame
        having ignored their resize."""
        self.assertLessEqual(commands_frame._SETTLE_SECONDS, 1.0)


class DensityIsWiredIntoTheCli(unittest.TestCase):
    """A palette row starts an argv; a CLI that does not accept it means the keypress
    silently does nothing, with no message anywhere to explain it."""

    def test_the_palettes_argv_shape_parses_and_dispatches(self):
        """Parsed from the argv `builtin_actions._run_density` actually starts, not from a
        hand-written
        one — a stored argv the CLI cannot parse is a hotkey that silently exits 2 inside
        a `run-shell` with nothing anywhere to print the reason.

        `assertIs` on the handler rather than patching it and calling: `set_defaults`
        binds the function OBJECT at parser-construction time, so a patch applied after
        `build_parser()` is not what `ns.func` holds — a test that patched and then
        asserted "called once" would be asserting against its own mock's identity, and
        passed just as well with the parser wired to the wrong command."""
        from charter.cli import build_parser
        argv = util.self_relaunch_argv("frame-density", "full")
        ns = build_parser().parse_args(argv[argv.index("charter") + 1:])
        self.assertEqual(ns.level, "full")
        self.assertIs(ns.func, commands_frame.cmd_density)

    def test_frame_density_is_reserved_against_a_harness_claiming_it(self):
        """`_add_frame_parsers` registers harness launchers BEFORE its own `frame-*`
        commands, so a harness with this `cli_name` would pass a check against
        `sub.choices` alone — nothing is named `frame-density` there yet — and only
        collide once that `add_parser` call ran a few lines later. On this repo's own
        3.11 floor `add_parser` accepts a duplicate name silently, so the harness would
        simply be shadowed and the palette would launch a harness instead of changing
        the frame's density. `_core_commands` reserves the word up front; this asserts on
        that guard's own wording, which is the half that does not depend on which
        interpreter runs the suite (see `test_frame_launcher.CollisionGuard` for the same
        argument spelled out at length)."""
        from charter import cli
        from charter.harness.base import Harness

        class _DensityClaimant(Harness):
            name = "density-claimant"
            cli_name = "frame-density"
            binary = "density-claimant"

        with mock.patch.dict("charter.harness.registry.KINDS",
                             {"density-claimant": _DensityClaimant}, clear=True):
            with self.assertRaises(ValueError) as ctx:
                cli.build_parser()
        self.assertIn("density-claimant", str(ctx.exception))
        self.assertIn("charter frame-density", str(ctx.exception))



if __name__ == "__main__":
    unittest.main()
