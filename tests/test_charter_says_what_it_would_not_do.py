"""Charter knowing a thing and not saying it, on the four surfaces that measured it.

Every case here is the same defect wearing a different hat: **charter established a fact,
acted on it, and reported success.** The four are filed separately because they fail on
four different surfaces, and one fix does not reach the others.

* **A refused `[[frame.component]]` arrangement** (#738). `instance.component_arrangement`
  throws the operator's whole arrangement away for one unusable value — which is #535's
  rule and stays — and until now the only trace was a `None` indistinguishable from "no
  arrangement was written". `charter frame-probe` printed a tick, `doctor`'s rows were
  green, and the frame that came up was the `slots` one, byte for byte identical to the
  frame the operator would have had if they had never written the tables.
* **`RefusedIsNotTheSameAsUndeclared`** is the half that keeps the fix honest. A warning
  that fires on correct configurations gets switched off and then protects nothing (#371,
  and `_BRANCH_MOVERS`' deletion), and *every* plane charter ships with — this repository's
  own included — declares no arrangement at all.
* **A `frame-*` command outside a frame** (#734). Six commands an operator can type opened
  with `if not fid: return 0`: no output, a success status, nothing done. Two of them are
  the *only* route to their action inside a tmux the operator already has, where charter
  binds no key.
* **What the frame is to drive** (#747). Nothing charter printed outside the frame named
  `F2`, `F12`, or that scrollback had become tmux's copy-mode.
* **The 3.2 resize limit** (#744). Both surfaces said a stretched frame stayed stretched
  "until the frame is relaunched". It stays stretched until you ask: `charter frame-resize`
  restores it in place, measured at the floor — see
  `tmuxctl.below_resize_hook_message`'s own docstring for the pane geometry.

Nothing here starts a tmux server. Every property is answerable from a resolved config, a
message constant, or a command's refusal branch, which is what makes them cheap enough to
assert one by one rather than through a launched frame — and `tests/test_frame_tmux_*`
already owns the other kind.
"""

from __future__ import annotations

import io
import os
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace
from unittest import mock

from charter import cli, commands_frame, doctor, instance
from charter.frame import component, overlay, state, tmuxctl
from tests import _envguard
from tests._isolation import PersonaIso
from tests.test_component_providers import CID, ENTRY, MODULE, _SitePackages, _source


def _frame(section: dict) -> dict:
    """`instance.frame_of` for a `[frame]` section written out as a dict.

    Through `frame_of` rather than `component_arrangement` directly, because the key this
    module is about is one `frame_of` sets and the readers below read: a test that called
    the resolver would assert the plumbing exists without asserting anything is plumbed
    into it.
    """
    return instance.frame_of({"frame": section})


class ARefusedArrangementNamesTheKeyThatDidIt(unittest.TestCase):
    """#738. One unusable value takes the whole arrangement out of play; the reason says
    which value, so the operator is not re-reading a committed file line by line."""

    def test_the_issues_own_bg_typo_is_named(self):
        """The reproduction as filed: four tables, one `bg = "midnight"`."""
        why = _frame({"component": [{"use": "identity"}, {"use": "attention"},
                                    {"use": "repos", "bg": "midnight"},
                                    {"use": "sidebar"}]})["components_refused"]
        self.assertIsNotNone(why)
        # The KEY, the VALUE and the COMPONENT — all three, because any two of them leave
        # a four-table file with more than one candidate line.
        self.assertIn("bg", why)
        self.assertIn("midnight", why)
        self.assertIn("repos", why)

    def test_the_arrangement_really_is_the_one_thrown_away(self):
        """The reason is only worth printing if it describes what happened. #535's rule is
        that the refusal is WHOLE, so the resolved frame must be the `slots` one — the same
        answer a plane that wrote nothing gets."""
        section = {"slots": ["top", "bottom"],
                   "component": [{"use": "identity"}, {"use": "repos", "bg": "midnight"}]}
        self.assertEqual(_frame(section)["slots"], ["top", "bottom"])
        self.assertEqual(_frame(section)["components"], [])

    def test_every_refusal_on_the_documented_list_has_a_reason(self):
        """`docs/frame.md` lists what is refused; a reason for some of them and a bare
        `None` for the rest would leave the operator guessing which kind they hit — and
        `frame_ready`/`doctor` would go silent on exactly the ones nobody thought of.

        Each row is a section that MUST refuse. The assertion is that no refusal comes back
        reasonless, which is the property, rather than the specific wording, which is not.
        """
        refusals = {
            "unknown use": [{"use": "nosuchthing"}],
            "misspelt key": [{"use": "repos", "bgg": "blue"}],
            "duplicate": [{"use": "repos"}, {"use": "repos"}],
            "no use at all": [{"edge": "top"}],
            "use is not a string": [{"use": 7}],
            "table is not a table": ["identity"],
            "visible is not a bool": [{"use": "repos", "visible": "yes"}],
            "bg outside the words": [{"use": "repos", "bg": "midnight"}],
            "pad above the max": [{"use": "repos", "pad": 99}],
            "pad below zero": [{"use": "repos", "pad": -1}],
            "edge on a built-in": [{"use": "repos", "edge": "left"}],
            "size charter cannot give": [{"use": "identity", "size": 4}],
            "key charter will not bind": [{"use": "repos", "key": "F2\nkill-server"}],
            "key already bound": [{"use": "repos", "key": "F2"}],
            "provider with no edge": [{"use": "weather", "size": 1}],
            "provider with no size": [{"use": "weather", "edge": "top"}],
        }
        for name, tables in refusals.items():
            with self.subTest(name):
                out = _frame({"component": tables})
                self.assertEqual(out["components"], [], f"{name} was not refused")
                self.assertTrue(out["components_refused"], f"{name} refused in silence")

    def test_a_committed_value_cannot_forge_a_line_of_the_report(self):
        """The reason is interpolated into `doctor`'s hint and `frame-probe`'s output, and
        every value in it arrives from a committed file that came off someone else's
        machine — `_HOTKEY_RE`'s own hazard, one key over. `contain.readable` is what
        stands between a `bg` holding a newline and a second row of charter's own report."""
        why = _frame({"component": [{"use": "repos", "bg": "blue\n  ✓  frame  all good"}]})
        self.assertNotIn("\n", why["components_refused"])
        self.assertIn("\\u000a", why["components_refused"])

    def test_a_hostile_use_is_contained_on_both_paths_that_print_it(self):
        """The `use` reaches the report twice and the two are different lines of code:
        `_component_at` names the table a refusal is about, and the provider branch names
        an id no distribution supplies. The deletion sweep found BOTH uncontained — a
        `bg` with a newline was pinned and a `use` with one was not, so the guard rested on
        the value the test happened to pick.

        A `use` is the likelier of the two, at that: it is the key an operator writing an
        arrangement types first, and `frame/component.py`'s own `_ID_RE` docstring records
        what a committed name that reaches tmux already cost once."""
        forged = "repos\n  ✓  frame             tmux 3.7"
        # `_component_at`: the id is good enough to be named, and a LATER key is refused.
        by_at = _frame({"component": [{"use": forged, "bg": "midnight"}]})
        # The provider branch: the id itself is what nothing supplies.
        by_provider = _frame({"component": [{"use": forged, "edge": "top", "size": 1}]})
        for name, out in (("_component_at", by_at), ("provider branch", by_provider)):
            with self.subTest(name):
                why = out["components_refused"]
                self.assertTrue(why)
                self.assertNotIn("\n", why)
                self.assertNotIn("✓", why)

    def test_a_value_is_quoted_the_way_its_own_file_spells_it(self):
        """`bg = "midnight"` is what is in the file; `bg = midnight` matches nothing an
        operator can search for. The quotes are a string's and only a string's — a `pad =
        99` that came back `pad = "99"` would send them looking for a line they did not
        write. The sweep found the branch unpinned: nothing failed when
        `contain.readable(value)` became the answer for both."""
        quoted = _frame({"component": [{"use": "repos", "bg": "midnight"}]})
        bare = _frame({"component": [{"use": "repos", "pad": 99}]})
        self.assertIn('`bg = "midnight"`', quoted["components_refused"])
        self.assertIn("`pad = 99`", bare["components_refused"])

    def test_a_use_charter_cannot_hash_is_refused_rather_than_raised(self):
        """**The one refusal here whose consequence is not a sentence.** `use = ["repos"]`
        and `use = {a = 1}` are both ordinary TOML, and both are unhashable — so without
        the `isinstance(cid, str)` line, `cid in seen` raises `TypeError` out of a function
        `config.derive` resolves OUTSIDE its try/except. That does not degrade to the
        default frame: it takes down `import charter.config`, and with it `charter
        --version` and every other command on that clone. The same cost this function's
        docstring already records for `Fixed`.

        Written because the sweep dropped that guard and stayed green: a hashable non-str
        `use` falls through to the provider branch and is refused there anyway, so the
        line looked like it merely chose which sentence came back."""
        for value in (["repos"], {"a": 1}, {1, 2}):
            with self.subTest(repr(value)):
                out = _frame({"component": [{"use": value}]})
                self.assertEqual(out["components"], [])
                self.assertTrue(out["components_refused"])

    def test_a_stray_key_is_contained_too_because_toml_keys_can_be_quoted(self):
        """A bare TOML key cannot hold a newline; a QUOTED one can — `"bg\\nx" = 1` is a
        legal table key — and this one is interpolated into `doctor`'s hint the same way a
        value is. The sweep found it uncontained: the `bg` case covered the values and
        nothing covered the keys."""
        why = _frame({"component": [{"use": "repos", "bg\n  ✓  frame": 1}]})
        self.assertTrue(why["components_refused"])
        self.assertNotIn("\n", why["components_refused"])
        self.assertNotIn("✓", why["components_refused"])

    def test_a_value_that_is_not_a_string_is_cut_to_a_length_a_row_can_hold(self):
        """`_component_value`'s other branch. A `pad` holding a four-thousand-element
        inline array is ordinary TOML and its `str()` is twenty kilobytes; unclipped it
        goes into a `doctor` hint and a `frame-probe` line whole. `contain.readable`'s
        limit is what stands between a committed file and a report nobody can read, and
        the sweep found nothing asserting it on this branch."""
        why = _frame({"component": [{"use": "repos", "pad": list(range(4000))}]})
        self.assertTrue(why["components_refused"])
        self.assertLess(len(why["components_refused"]), 500)

    def test_a_table_with_no_usable_use_is_named_by_its_position(self):
        """`_component_at`'s other branch. When the broken key IS the `use`, there is no
        name to quote back and the ordinal is all the operator has to find the table by —
        counting from 1, because that is how the file reads. The sweep collapsed the
        conditional and nothing noticed: every other case in this module has a usable
        `use` for it to fall back on."""
        why = _frame({"component": [{"use": "identity"}, {"edge": "top"}]})
        self.assertIn("table 2", why["components_refused"])

    def test_a_hostile_use_never_reaches_the_duplicate_message(self):
        """The argument that lets the duplicate message interpolate its `use` raw, written
        as a test rather than left in a comment. `seen` only ever holds an id that
        completed a whole pass of the loop, so a `use` that needs containing is refused on
        its FIRST occurrence — by the branch that says no distribution supplies it, not by
        the one that says it is placed twice. Reorder those and this goes red."""
        forged = "repos\n  ✓  frame             tmux 3.7"
        why = _frame({"component": [{"use": forged}, {"use": forged}]})["components_refused"]
        self.assertIn("no installed distribution supplies it", why)
        self.assertNotIn("placed twice", why)
        self.assertNotIn("\n", why)

    def test_the_stray_key_named_is_the_first_one_down_the_file(self):
        """`tomllib` hands back keys in the order they were written, and the operator is
        about to go looking for one. Alphabetical was the first spelling and the sweep
        found `sorted` and `list` indistinguishable — asked again from the operator's side,
        file order is also the better answer."""
        why = _frame({"component": [{"use": "repos", "zzz": 1, "aaa": 2}]})
        self.assertIn("`zzz`", why["components_refused"])
        self.assertNotIn("`aaa`", why["components_refused"])


class OneCellIsASizeAndZeroIsNot(unittest.TestCase):
    """The provider branch's `size < 1`, from both sides.

    Splitting one `or` chain into three sentences made the boundary its own line, and the
    sweep immediately shifted it: `size <= 1` passed the whole suite, because every case
    that reached this branch was refused for a different reason first — no installed
    distribution supplied the id. A boundary only one side of which is exercised is not a
    boundary that has been tested, so this class installs a real distribution and asks for
    the smallest size charter will honour as well as the largest it will not.
    """

    def setUp(self) -> None:
        _envguard.unset_all()
        site = _SitePackages(self)
        site.install("acme-charter", "1.0", {CID: ENTRY}, {MODULE: _source()})

    def test_one_cell_is_placed(self):
        out = _frame({"component": [{"use": CID, "edge": "right", "size": 1}]})
        self.assertIsNone(out["components_refused"])
        self.assertEqual([p["size"] for p in out["components"]], [component.Fixed(1)])

    def test_zero_cells_is_refused_and_says_so(self):
        why = _frame({"component": [{"use": CID, "edge": "right", "size": 0}]})
        self.assertEqual(why["components"], [])
        self.assertIn("`size = 0`", why["components_refused"])


class RefusedIsNotTheSameAsUndeclared(unittest.TestCase):
    """The half of #738 that keeps it from becoming #371.

    `_BRANCH_MOVERS` is this repository's record of a guard that fired too often being
    deleted outright, and a warning an operator learns to ignore protects nothing. The
    reason must be `None` for every plane that is *working*, and the population of those is
    "every plane that did not write the key" — which is every plane charter ships.
    """

    def test_a_plane_that_declared_no_arrangement_says_nothing(self):
        for name, cfg in {"no [frame] section": {},
                          "[frame] with other keys": {"frame": {"slots": ["top"]}},
                          "a density preset": {"frame": {"density": "minimal"}}}.items():
            with self.subTest(name):
                self.assertIsNone(instance.frame_of(cfg)["components_refused"])

    def test_an_arrangement_charter_accepts_says_nothing(self):
        out = _frame({"component": [{"use": "identity"}, {"use": "repos", "bg": "blue"},
                                    {"use": "sidebar", "key": "F9", "pad": 1}]})
        self.assertIsNone(out["components_refused"])
        self.assertEqual(len(out["components"]), 3)

    def test_this_repositorys_own_plane_says_nothing(self):
        """The sharpest form of the same test, and the one that would have caught a fix
        that warned on a working file: charter's own `charter.toml` is a real, committed,
        hand-maintained plane config, and it must not put `doctor` in the yellow."""
        import tomllib
        from pathlib import Path
        here = Path(__file__).resolve().parent.parent / "charter.toml"
        cfg = tomllib.loads(here.read_text())
        self.assertIsNone(instance.frame_of(cfg)["components_refused"])

    def test_the_key_is_present_on_every_path_frame_of_returns_by(self):
        """`frame_of`'s own rule for `components`: a key on one path and absent on another
        is two shapes for one answer, and the reader would be catching a `KeyError` on
        exactly the planes that declare no `[frame]` section."""
        for cfg in ({}, {"frame": "not a table"}, {"frame": {}},
                    {"frame": {"component": [{"use": "identity"}]}},
                    {"frame": {"component": [{"use": "repos", "bg": "midnight"}]}}):
            with self.subTest(repr(cfg)):
                self.assertIn("components_refused", instance.frame_of(cfg))


class BothSurfacesSayIt(unittest.TestCase):
    """#738's actual complaint: three surfaces whose job is to report configuration health,
    all reporting health. The launch stays silent on purpose (`frame_ready`'s docstring
    measures why), so these two are the whole surface."""

    def setUp(self) -> None:
        _envguard.unset_all()
        self.refused = _frame({"component": [{"use": "repos", "bg": "midnight"}]})

    def test_frame_probe_names_it_as_a_standing_limit(self):
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch.dict(commands_frame.config.FRAME, self.refused, clear=False):
            code, level, line = commands_frame.frame_ready()
        self.assertEqual(level, "warn")
        self.assertIn("[[frame.component]]", line)
        self.assertIn("midnight", line)
        # Not an error, for the reason none of the other ceilings are: `cmd_launch` draws
        # the frame regardless, and a probe stricter than the launcher lies about it.
        self.assertEqual(code, 0)

    def test_a_clean_plane_leaves_the_probe_a_tick(self):
        """The #371 half at this surface. It held before the fix and has to go on holding
        after it: a probe that warned about a working arrangement would be the ceiling list
        learning to cry wolf, and the whole list gets ignored together."""
        clean = _frame({"component": [{"use": "identity"}]})
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch.dict(commands_frame.config.FRAME, clean, clear=False):
            code, level, line = commands_frame.frame_ready()
        self.assertEqual((code, level), (0, "ok"))
        self.assertNotIn("[[frame.component]]", line)

    def test_doctor_puts_it_where_the_other_ignored_keys_are(self):
        """`charter.toml`, not `frame` — beside `[plane] worktrees` and `[harness] default`
        (#715), because it is the same fact as those two: a committed setting that is not
        in force and reads exactly like one that was never written. `check_frame` answers
        "can a frame run on this MACHINE"."""
        with mock.patch("charter.instance.frame_of", return_value=self.refused):
            r = doctor.check_control_plane_config()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("[[frame.component]]", r.detail)
        self.assertIn("midnight", r.hint)

    def test_both_surfaces_say_it_in_the_same_words(self):
        """One sentence, shared. Two copies of a standing fact drift into two different
        facts — `no_renderer_message`'s own reason for existing, and `below_floor_message`'s
        history before it."""
        why = self.refused["components_refused"]
        sentence = instance.refused_arrangement_message(why)
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch.dict(commands_frame.config.FRAME, self.refused, clear=False):
            _code, _level, line = commands_frame.frame_ready()
        with mock.patch("charter.instance.frame_of", return_value=self.refused):
            r = doctor.check_control_plane_config()
        self.assertIn(sentence, line)
        self.assertIn(sentence, r.hint)


class AFrameCommandOutsideAFrameSaysSo(unittest.TestCase):
    """#734, and the class turned out wider than the four commands it names.

    `frame-switch` is the fifth — the ONLY one of them with a `help=` string, so it is the
    one `charter --help` lists and the most likely to be typed by somebody who has never
    been in a frame. `frame-resize` is the sixth, and #744 is what made it one: below tmux
    3.3 it is the operator's only resize recovery, and both surfaces now tell them to type
    it.
    """

    #: Every `frame-*` command an operator can type, with an argv that is otherwise valid.
    #: The point of "otherwise valid" is that nothing else in the command can be blamed for
    #: the silence — `repos` IS in this plane's arrangement, `minimal` IS a density level.
    TYPEABLE = {
        "charter frame-chat": (commands_frame.cmd_chat,
                               SimpleNamespace(chat_id="alpha.1", chat="")),
        "charter frame-density": (commands_frame.cmd_density,
                                  SimpleNamespace(level="minimal")),
        "charter frame-toggle": (commands_frame.cmd_toggle,
                                 SimpleNamespace(component="repos", chat="")),
        "charter frame-chrome": (commands_frame.cmd_chrome, SimpleNamespace(level="dark")),
        "charter frame-switch": (commands_frame.cmd_switch,
                                 SimpleNamespace(workspace="beta", persona=None)),
        "charter frame-resize": (commands_frame.cmd_resize, SimpleNamespace(frame=None)),
    }

    def setUp(self) -> None:
        # The whole condition under test is "no `$CHARTER_SESSION_ID`", and the shell the
        # suite was launched from may well be inside a live frame — which is exactly how
        # this issue's own reproduction was first run against the wrong branch.
        _envguard.unset_all()

    def test_each_one_says_which_command_did_nothing(self):
        for name, (fn, args) in self.TYPEABLE.items():
            with self.subTest(name):
                err = io.StringIO()
                with redirect_stderr(err):
                    rc = fn(args)
                self.assertNotEqual(rc, 0, f"{name} reported success having done nothing")
                self.assertIn(name, err.getvalue())

    def test_the_sentence_says_where_to_run_it_instead(self):
        """"Not in a frame" tells an operator who typed this in the wrong pane nothing they
        did not already know. The window is the actionable half, and `charter docs show
        frame` is where the rest of it lives (#747)."""
        err = io.StringIO()
        with redirect_stderr(err):
            commands_frame.cmd_toggle(SimpleNamespace(component="repos", chat=""))
        said = err.getvalue()
        self.assertIn("charter <harness>", said)
        self.assertIn("charter docs show frame", said)

    def test_the_commands_tmux_fires_for_itself_stay_quiet_at_zero(self):
        """The exit code is not free. `run-shell` prints `'<the whole command>' returned 1`
        into the harness pane — the one rectangle ADR 0018 says charter never draws in —
        so a command tmux drives must not start doing that. Neither of these is ever typed:
        `frame-respawn` is a `pane-died` hook and `frame-gather` a detached child of the
        launcher. (`frame-palette` is the third of that set and is not driven here — it has
        no early return to assert against; it opens a palette whatever it finds, which is
        the one of the nine that was never in this class.)"""
        for name, fn, args in (
                ("frame-respawn", commands_frame.cmd_respawn,
                 SimpleNamespace(slot="repos", pane="%9", frame=None)),
                ("frame-gather", commands_frame.cmd_gather,
                 SimpleNamespace(session="", workspace="beta"))):
            with self.subTest(name):
                err = io.StringIO()
                with redirect_stderr(err):
                    rc = fn(args)
                self.assertEqual(rc, 0)
                self.assertEqual(err.getvalue(), "")


class InsideAFrameTheRefusalGoesOnTheFrame(PersonaIso, unittest.TestCase):
    """The other half of #734's class, and the half that had no affordable surface until
    #729 built one.

    `outside_a_frame` answers the operator who is not in a frame. These three are the
    operator who IS, and typed an argument charter does not know: `charter frame-density
    enormous`, `charter frame-chrome solarized`, `charter frame-toggle reposs`. Each did
    the same nothing at rc 0 — the same defect, one branch over, and it was left alone in
    the first pass of this work for two reasons that #729 has since removed.

    The surface was `display-message`, so saying anything meant putting a typed argument
    through a tmux FORMAT evaluator, and `cmd_toggle`'s guard is the one thing standing
    between an argv word and a `split-window` target (`test_component_toggle_keys.py`'s
    hostile-name class). It also meant the message landed on whichever client tmux picked,
    which on an eleven-frame socket is somebody else's terminal. The attention row is
    charter's own pane, written through `state.say`'s `contain.one_line` and read by this
    frame's own panel, so neither objection survives — and the exit status stays 0, which
    is what a `run-shell` child owes tmux.
    """

    def setUp(self) -> None:
        super().setUp()
        self.fid = "knows-1"
        self.enterContext(mock.patch.dict(os.environ,
                                          {"CHARTER_SESSION_ID": self.fid}))
        self.enterContext(mock.patch("charter.frame.tmuxctl.version",
                                     return_value=(3, 7)))
        state.record_harness_pane(self.fid, "%0")
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})

    def _tmux_calls(self):
        """Every `tmuxctl.run` the command made. The row is state plus a version bump, so
        a refusal that reached tmux at all would be one that moved a pane."""
        calls = []
        return calls, mock.patch("charter.frame.tmuxctl.run",
                                 side_effect=lambda *a, **k: calls.append(a))

    def test_a_density_level_charter_does_not_know_says_which_three_it_does(self):
        calls, patched = self._tmux_calls()
        with patched:
            rc = commands_frame.cmd_density(SimpleNamespace(level="enormous"))
        self.assertEqual(rc, 0, "a run-shell child still owes tmux a zero")
        self.assertIn("enormous", state.notice(self.fid))
        for level in instance.FRAME_DENSITY:
            self.assertIn(level, state.notice(self.fid))
        self.assertEqual(calls, [], "a refused level moved something")

    def test_a_chrome_level_charter_does_not_know_says_which_it_does(self):
        calls, patched = self._tmux_calls()
        with patched:
            rc = commands_frame.cmd_chrome(SimpleNamespace(level="solarized"))
        self.assertEqual(rc, 0)
        self.assertIn("solarized", state.notice(self.fid))
        self.assertEqual(calls, [])

    def test_a_component_this_arrangement_does_not_hold_says_which_it_does(self):
        """It also answers a live frame's dead key. A component's `bind -n` outlives an
        edit to `charter.toml` that drops the component, so this is the only thing that
        tells an operator why a key they configured has stopped doing anything."""
        calls, patched = self._tmux_calls()
        with patched:
            rc = commands_frame.cmd_toggle(SimpleNamespace(component="reposs", chat=""))
        self.assertEqual(rc, 0)
        self.assertIn("reposs", state.notice(self.fid))
        self.assertEqual(calls, [])

    def test_a_level_that_renders_as_nothing_is_still_named(self):
        """`state.say` already runs `contain.one_line`, so the newline half is covered
        before these ever call it — which is exactly why the sweep found the extra
        `contain.readable` unpinned and why the difference had to be MEASURED rather than
        reasoned about.

        The two differ on **invisible codepoints**, and `contain.readable`'s own docstring
        says why: `one_line` decides on `_INVISIBLE`, a list of five categories, and
        U+3164 HANGUL FILLER and U+2800 BRAILLE PATTERN BLANK are on none of them, are not
        `isspace`, and survive `strip`. Measured through this exact path, `charter
        frame-density <three U+3164>`:

            with    `contain.readable`: charter: no density level \\u3164\\u3164\\u3164 — have: …
            without it:                 charter: no density level ㅤㅤㅤ — have: …

        The second draws as `no density level    — have: …` — a refusal naming NO level,
        which is #498's defect one surface over and the precise opposite of what this
        message exists to do. Both commands, because they are two lines, and the sweep
        reported them as two survivors."""
        for name, fn, field in (
                ("frame-density", commands_frame.cmd_density, "density level"),
                ("frame-chrome", commands_frame.cmd_chrome, "chrome level")):
            with self.subTest(name):
                state.say(self.fid, "", seconds=0.0)
                fn(SimpleNamespace(level="\u3164\u3164\u3164"))
                said = state.notice(self.fid)
                self.assertIn(field, said)
                self.assertIn("\\u3164", said,
                              "an invisible level reached the row as nothing at all")

    def test_a_hostile_component_name_is_contained_and_still_travels_no_further(self):
        """The guard is unchanged and only its silence went. The name reaches a sentence
        and nothing else — no `split-window`, no hook action text — and the sentence is one
        line, because `state.say` runs `contain.one_line` over the assembled row."""
        calls, patched = self._tmux_calls()
        with patched:
            commands_frame.cmd_toggle(
                SimpleNamespace(component="repos\nkill-server", chat=""))
        said = state.notice(self.fid)
        self.assertTrue(said)
        self.assertNotIn("\n", said)
        self.assertNotIn("kill-server", said.replace("\\u000akill-server", ""))
        self.assertEqual(calls, [])


class TheHelpSaysHowToDriveIt(unittest.TestCase):
    """#747. `charter claude --help` and `charter frame --help` described four flags and
    never said what the frame they open is, or how to work it."""

    def setUp(self) -> None:
        _envguard.unset_all()

    def _epilog(self, command: str) -> str:
        parser = cli.build_parser()
        return parser._subparsers._group_actions[0].choices[command].epilog or ""

    def test_every_launcher_names_the_palette_the_hatch_and_scrollback(self):
        """Both, and by construction: `cli._wire` sets one epilog for every registered
        harness plus the escape hatch, so a harness registered next year gets it too."""
        for command in ("claude", "frame"):
            with self.subTest(command):
                epilog = self._epilog(command)
                self.assertIn(commands_frame.config.FRAME["hotkey"], epilog)
                self.assertIn(overlay.HATCH_KEY, epilog)
                self.assertIn("copy-mode", epilog)
                self.assertIn("charter docs show frame", epilog)

    def test_the_two_launchers_still_say_the_same_thing(self):
        """#747 observed that the two help pages are identical and did not ask for that to
        change — what was wrong is that the identical thing they said was silent on the
        frame. `assertTrue` first, because equality alone was satisfied on the day both
        epilogs were `None`: an assertion that holds over the defect it is guarding is one
        this project has paid for repeatedly."""
        self.assertTrue(self._epilog("claude"))
        self.assertEqual(self._epilog("claude"), self._epilog("frame"))

    def test_the_palette_key_is_this_planes_own_and_not_the_constant(self):
        """A help line saying `F2` on a plane that moved `[frame] hotkey` is worse than one
        that says nothing: it is a fact the operator will act on, once, and then stop
        trusting the page."""
        with mock.patch.dict(commands_frame.config.FRAME, {"hotkey": "C-b"}, clear=False):
            keys = " ".join(commands_frame.driving_keys())
        self.assertIn("C-b", keys)
        self.assertNotIn("F2 opens", keys)

    def test_the_probe_says_it_too_ceiling_or_no_ceiling(self):
        """`frame-probe` is the closest thing charter has to "tell me about the frame", and
        it named every standing limit and no key at all. It rides below the ceilings rather
        than joining them: a ceiling is a capability this machine does not have, and a key
        that works is the opposite of one."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)):
            _code, level, line = commands_frame.frame_ready()
        self.assertEqual(level, "ok")
        self.assertIn(overlay.HATCH_KEY, line)
        self.assertIn("copy-mode", line)


class TheThirtyTwoResizeLimitNamesItsRemedy(unittest.TestCase):
    """#744, measured on `~/.local/share/charter-testing/tmux-3.2`: a frame launched at
    120x40, dragged to 80x24 and back, reports `%1 5x120 %0 22x97 %4 22x22 %3 5x120
    %2 5x120` and keeps them; `charter frame-resize` puts back `%1 1x120 %0 34x97 %4 34x22
    %3 1x120 %2 1x120`, which is the launch geometry exactly.

    So the standing limit is not "until the frame is relaunched" — it is "until you ask",
    and asking is one command that already exists.
    """

    def setUp(self) -> None:
        # `doctor.check_frame` asks `_statusline_suppressed_note` whether THIS session's
        # footer is being blanked, which reads `$CHARTER_SESSION_ID`. Outside a frame is
        # the state these assertions are about, and it is stated rather than inherited.
        _envguard.unset_all()

    def test_the_message_stops_promising_a_relaunch(self):
        said = tmuxctl.below_resize_hook_message((3, 2))
        self.assertNotIn("until the frame is relaunched", said)
        self.assertIn("charter frame-resize", said)

    def test_it_stops_saying_everything_else_works(self):
        """"Panels stretch" reads as cosmetic. At 80x24 on 3.2 the sidebar is two columns
        of glyph stubs and the repo pane holds a permanent `⋯ too narrow for the repo
        table` — a line written to be transient, which never settles because nothing
        re-measures."""
        said = tmuxctl.below_resize_hook_message((3, 2))
        self.assertIn("too narrow for the repo table", said)
        self.assertIn("sidebar", said)

    def test_both_reading_surfaces_carry_the_remedy(self):
        """`frame_ready` and `doctor.check_frame` share the one sentence rather than
        writing it twice — the reason `below_floor_message` was extracted in the first
        place."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 2)):
            _code, level, line = commands_frame.frame_ready()
            row = doctor.check_frame()
        self.assertEqual(level, "warn")
        self.assertIn("charter frame-resize", line)
        self.assertEqual(row.status, doctor.WARN)
        self.assertIn("charter frame-resize", row.hint)

    def test_a_tmux_above_the_floor_is_told_none_of_it(self):
        """The limit is real on 3.2 and absent on 3.3+; a remedy offered to somebody who
        does not need it is the noise that gets a whole surface ignored."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)):
            _code, _level, line = commands_frame.frame_ready()
        self.assertNotIn("charter frame-resize", line)


if __name__ == "__main__":
    unittest.main()
