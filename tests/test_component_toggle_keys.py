"""A component has its own key, and pressing it hides or shows that component alone.

Density was all-or-nothing: three level names, each expanding to a whole frame. The
operator's own words for what was missing — *"instead of having density - we need to have
hotkeys to hide and show separately components"*.

So **visibility is the mechanism and density is a name for one arrangement of it**
(spec §4). One key per component, declared beside the component in `[[frame.component]]`,
bound by `commands_frame.conf_text` at launch, and firing `charter frame-toggle <name>`
— which flips that one name in the frame's own hidden set and hands the result to the
SAME re-layout a density change goes through. There is no second resize path, and
`cmd_density` is now a caller of the same one rather than the owner of it.

Four properties, one class each:

**A key is validated at the config boundary, by `instance._HOTKEY_RE`.** Not a second
regex. `[frame] hotkey` reached tmux CONFIG TEXT and a newline in it ran a second command
at launch with no keypress (`instance._HOTKEY_RE`'s own docstring measures it); a
component's key reaches the identical `bind -n` line from the identical committed file,
so it is held to the identical alphabet. `TheKeyIsValidatedWhereTheHotkeyIs` asks that as
a property of the two functions, not of a spelling.

**A key toggles ONE component, live, and the layout recomputes.**
`AKeyTogglesOneComponentLive` is Task 5's headline: the pane that goes is the toggled
component's, every other pane survives with its size re-asserted, and the panels repaint
because the frame's version bumped.

**Density is a named arrangement over that same visibility.**
`DensityIsANamedArrangementOverVisibility` pins that a level writes a hidden SET rather
than driving a layout of its own, so a toggle after a density change composes with it
instead of fighting it.

**Nothing off disk reaches tmux unchecked.** The hidden set is a file under the frame's
own state directory, which is to say a file whoever can write there decides the contents
of (#475's whole shape) — so a name read back out of it is held to the component
alphabet, and a name that is not in this frame's arrangement toggles nothing.
"""

from __future__ import annotations

import os
import subprocess
import sys
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import component, overlay, state

from tests._isolation import PersonaIso


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped — `tests/test_frame_density.py`'s own
    helper, repeated rather than imported for the reason that copy states: a test module
    importing another test module's private helper couples two files that are otherwise
    independent."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


class _Tmux:
    """A recording stand-in for `tmuxctl.run` — `tests/test_frame_density.py`'s fake,
    repeated for the same reason `_a_dead_pid` is. It answers the two queries a re-layout
    actually reads a value out of (the window size, and a pane id per split) and reports
    success for everything else."""

    def __init__(self, *, size="200:50", new_panes=("%7", "%8", "%9")):
        self.size = size
        self.new_panes = list(new_panes)
        self.calls: list[list[str]] = []

    def __call__(self, action, argv, *, env=None, timeout=None, report=True):
        self.calls.append(list(argv))
        out = ""
        if "display-message" in argv:
            out = self.size
        elif "split-window" in argv:
            out = self.new_panes.pop(0) if self.new_panes else ""
        return subprocess.CompletedProcess(argv, 0, stdout=out, stderr="")

    def killed(self) -> set[str]:
        return {c[c.index("kill-pane") + 2] for c in self.calls if "kill-pane" in c}

    def split(self) -> list[str]:
        return [c[c.index("panel") + 1] for c in self.calls
                if "split-window" in c and "panel" in c]


#: The arrangement every live test below runs against: charter's own four panels, written
#: out the long way so each can carry a key. Spelled as committed TOML tables rather than
#: as resolved placements, because the thing under test is what a COMMITTED file does —
#: an arrangement assembled by hand into the shape `layout` wants would pass with
#: `instance.frame_of` dropping the key on the floor.
_TABLES = [
    {"use": "identity", "key": "F5"},
    {"use": "attention", "key": "F6"},
    {"use": "repos", "key": "F7"},
    {"use": "sidebar", "key": "F8"},
]


class AKeyTogglesOneComponentLive(PersonaIso, unittest.TestCase):
    """Task 5's headline: a component's key toggles only it, live, and the layout
    recomputes — through the Phase 1 registry's own path, not a second one."""

    def setUp(self):
        super().setUp()
        self.fid = f"tg-{_a_dead_pid()}"
        self.enterContext(mock.patch.dict(os.environ,
                                          {"CHARTER_SESSION_ID": self.fid}))
        self.enterContext(mock.patch("charter.frame.tmuxctl.version",
                                     return_value=(3, 7)))
        self.frame = instance.frame_of({"frame": {"component": list(_TABLES)}})
        self.enterContext(mock.patch.dict(config.FRAME, self.frame))
        state.record_harness_pane(self.fid, "%0")
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2",
                                             "repos": "%5", "right": "%4"})

    def _toggle(self, name, fake=None):
        fake = fake or _Tmux()
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            rc = commands_frame.cmd_toggle(SimpleNamespace(component=name))
        return rc, fake

    def test_the_arrangement_carries_every_declared_key(self):
        """The control for everything below: the committed key survives `frame_of`. A
        run where it did not would pass the no-op assertions for the wrong reason."""
        self.assertEqual({p["slot"]: p["key"] for p in self.frame["components"]},
                         {"top": "F5", "bottom": "F6", "repos": "F7", "right": "F8"})

    def test_pressing_a_key_kills_that_components_pane_and_no_other(self):
        before = state.version(self.fid)
        rc, fake = self._toggle("repos")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.killed(), {"%5"}, fake.calls)
        self.assertEqual(state.panes(self.fid),
                         {"top": "%1", "bottom": "%2", "right": "%4"})
        self.assertGreater(state.version(self.fid), before)

    def test_pressing_it_again_brings_that_component_back_and_only_it(self):
        """A toggle is not a delete: the arrangement still holds `repos`, so the second
        press has something to bring back and knows its edge and its size without the
        operator naming any of it again. That is the whole difference from deleting a name
        from `slots`, which loses the position along with the panel.

        The recorded map is survivors-then-new, and that is `_relayout`'s own documented
        behaviour rather than anything this changes: a pane that did not change is not
        re-split (#500 round 3 measures why the surviving order is what `repos_cols` must
        be asked with). Where the ARRANGEMENT's order shows up is in `want`, which is what
        a re-layout from nothing splits in — `DensityIsANamedArrangementOverVisibility`
        asserts that half."""
        self._toggle("repos")
        rc, fake = self._toggle("repos")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.split(), ["repos"], fake.calls)
        self.assertEqual(fake.killed(), set(), fake.calls)
        self.assertEqual(list(state.panes(self.fid)),
                         ["top", "bottom", "right", "repos"])
        # Recorded EMPTY, not forgotten: "the operator turned the last one back on" and
        # "nobody has touched this frame" are different answers, and only the second one
        # falls back to what the config declared.
        self.assertEqual(state.hidden(self.fid), ())

    def test_a_first_press_never_conjures_a_panel_the_frame_was_not_showing(self):
        """A plane whose `[frame] slots` was trimmed to two panels shows two panels, and a
        keypress may only ever change the ONE component it names. The universe a level can
        reach is longer than the frame — that is what lets `full` still mean four panels —
        and reading that longer list as "everything is visible" would make the first press
        of any key add the two the operator had removed."""
        frame = instance.frame_of({"frame": {"slots": ["top", "bottom"]}})
        with mock.patch.dict(config.FRAME, frame):
            state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2"})
            rc, fake = self._toggle("top")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.split(), [], fake.calls)
        self.assertEqual(fake.killed(), {"%1"}, fake.calls)
        self.assertEqual(state.panes(self.fid), {"bottom": "%2"})

    def test_the_survivors_keep_their_sizes(self):
        """tmux redistributes every remaining pane proportionally on a `kill-pane`, so a
        toggle that only killed a pane would leave the others stretched. This is the same
        `_reassert_sizes` a density change goes through — which is the point: one path."""
        _, fake = self._toggle("repos")
        resized = {c[c.index("-t") + 1] for c in fake.calls if "resize-pane" in c}
        self.assertIn("%1", resized)
        self.assertIn("%2", resized)
        self.assertIn("%4", resized)


class ARefusedToggleChangesNothingAtAll(PersonaIso, unittest.TestCase):
    """The refusals `cmd_toggle` makes, each asserted by what did NOT happen.

    A return code cannot tell these apart — `cmd_toggle` answers 0 for everything, on
    purpose, because it runs as a `run-shell` child where the only screen left to report
    on is the agent's own. So each test below asserts the CONSEQUENCE the guard exists to
    prevent: no tmux command ran, and the frame's hidden set is untouched. A guard deleted
    is then a tmux call that appears, not an exit code that stays the same.
    """

    def setUp(self):
        super().setUp()
        self.fid = f"tg-{_a_dead_pid()}"
        self.enterContext(mock.patch("charter.frame.tmuxctl.version",
                                     return_value=(3, 7)))
        self.enterContext(mock.patch.dict(
            config.FRAME, instance.frame_of({"frame": {"component": list(_TABLES)}})))
        state.record_harness_pane(self.fid, "%0")
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2",
                                             "repos": "%5", "right": "%4"})

    def _toggle(self, name, *, fid=None):
        env = {} if fid is None else {"CHARTER_SESSION_ID": fid}
        fake = _Tmux()
        with mock.patch.dict(os.environ, env, clear=True), \
             mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            rc = commands_frame.cmd_toggle(SimpleNamespace(component=name))
        return rc, fake

    def test_a_name_this_frames_arrangement_does_not_hold_moves_no_pane(self):
        """`made_up` is a perfectly usable component id and is not in this arrangement,
        so the ONLY guard that can refuse it is the arrangement check. Without it the name
        goes on to `_relayout` and becomes a `split-window ... charter panel made_up` and
        a respawn hook naming it in tmux config text."""
        rc, fake = self._toggle("made_up", fid=self.fid)
        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [])
        self.assertIsNone(state.hidden(self.fid))

    def test_a_hostile_name_is_refused_by_the_same_one_guard(self):
        """The `[frame] hotkey` class, arriving on an argv instead of in a config file.
        Not reasoned about — each of these is run.

        The consequence a survivor would have: `_arm_panel_respawn` writes the name into
        a tmux hook's ACTION TEXT, which tmux re-parses, and `layout.panel_command` puts
        it on a `split-window` argv. A newline there is `instance._HOTKEY_RE`'s own
        incident one surface over."""
        for name in ("repos\nkill-server", "repos kill-server", "repos\x1b[31m",
                     "repos'", 'repos"', "repos#{q:x}", "repos;kill-server",
                     "repos $(id)", "../../etc/passwd", "REPOS", ""):
            with self.subTest(name=name):
                rc, fake = self._toggle(name, fid=self.fid)
                self.assertEqual(rc, 0)
                self.assertEqual(fake.calls, [], name)
                self.assertIsNone(state.hidden(self.fid))

    def test_a_real_name_is_the_control_and_does_move_a_pane(self):
        """Without this, a `cmd_toggle` that refused everything would pass every
        assertion above."""
        rc, fake = self._toggle("repos", fid=self.fid)
        self.assertEqual(rc, 0)
        self.assertEqual(fake.killed(), {"%5"}, fake.calls)
        self.assertEqual(state.hidden(self.fid), ("repos",))

    def test_outside_a_frame_it_does_nothing(self):
        """No `$CHARTER_SESSION_ID` — typed in an ordinary shell rather than fired by a
        bind. Nothing runs and nothing is written.

        **`cmd_toggle` has no refusal of its own for this, and that is a finding rather
        than an omission.** The deletion sweep proved an `if not fid: return 0` at the top
        of that function exactly equivalent: with it gone the full suite stayed green and
        every observable was identical — same return code, no tmux command, no file
        created, nothing readable back. The line that actually carries the property is
        `_relayout_target`'s `_PANE_ID_RE` check, because `contain.child` refuses `""` as a
        path segment, so `state.frame_dir` and `state.harness_pane` both answer `None` and
        the pane id is the empty string. That line IS pinned
        (`test_a_harness_pane_that_is_not_tmuxs_own_shape_moves_nothing`), so the guarantee
        rests on something a mutation can reach — which is the whole test the sweep applies.

        The chain is asserted here, not inferred: if `frame_dir` ever started answering for
        an empty id, this test says so at the step where it changed."""
        self.assertIsNone(state.frame_dir(""))
        self.assertIsNone(state.harness_pane(""))
        rc, fake = self._toggle("repos")
        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [])
        self.assertIsNone(state.hidden(self.fid))

    def test_a_harness_pane_that_is_not_tmuxs_own_shape_moves_nothing(self):
        """#475's rule, on the value `_relayout_target` reads off disk. Unrefused, `%1;
        kill-server` becomes a split target and a hook action — which is the file-in-the-
        state-directory attack that guard was added for.

        The control is the test above: the same call with a real `%0` recorded does move a
        pane, so a `cmd_toggle` that had simply stopped working is red."""
        for pane in ("%1;kill-server", "", "1", "%", "@1"):
            with self.subTest(pane=pane):
                state.record_harness_pane(self.fid, pane)
                rc, fake = self._toggle("repos", fid=self.fid)
                self.assertEqual(rc, 0)
                self.assertEqual(fake.calls, [], pane)
                self.assertIsNone(state.hidden(self.fid))

    def test_a_tmux_whose_version_charter_could_not_read_moves_nothing(self):
        """Every builder a re-layout reaches for takes the version — `_relayout` decides
        `split-window`'s flags from it. Without a version there is nothing to build the
        commands from, so the frame is left exactly as it is."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=None):
            rc, fake = self._toggle("repos", fid=self.fid)
        self.assertEqual(rc, 0)
        self.assertEqual(fake.calls, [])
        self.assertIsNone(state.hidden(self.fid))

    def test_the_hidden_set_is_written_only_after_the_frame_can_be_relaid_out(self):
        """Ordering, asserted rather than assumed. A refusal that had already written the
        hidden set would leave the frame's recorded shape disagreeing with its panes for
        the rest of its life — and the next thing that DID re-lay it out would silently
        apply a decision this keypress was refused."""
        state.record_harness_pane(self.fid, "%1;kill-server")
        self._toggle("repos", fid=self.fid)
        self.assertIsNone(state.hidden(self.fid))


class TheKeyIsValidatedWhereTheHotkeyIs(unittest.TestCase):
    """`[[frame.component]]`'s `key` is `[frame] hotkey`'s alphabet, asked once.

    Not "a similar check". `instance.toggle_key` IS `instance._HOTKEY_RE`, and the first
    test below asks that as a property of the two functions rather than by reading the
    pattern — a second regex spelled beside the first would pass an eyeball review and
    drift on the first change to either.
    """

    def test_a_toggle_key_and_a_frame_hotkey_accept_exactly_the_same_words(self):
        """The two answer identically for every word, accepted and refused alike. A
        second pattern — even a correct copy of today's — is red the day either moves."""
        for word in ("F2", "F12", "M-m", "C-M-S-x", "a", "7", "Up", "PPage", "BSpace",
                     "Escape", "|", "_", "-",
                     "F2\nrun-shell 'touch /tmp/PWNED'", "F2 ", " F2", "F2;kill-server",
                     "F2#{q:x}", "F2$(id)", 'F2"', "F2'", "", "F2 x", "M-",
                     "C-C-C-C-x", "toolongforakeynametobeatallplausible"):
            with self.subTest(word=word):
                self.assertEqual(instance.toggle_key(word) is not None,
                                 instance._HOTKEY_RE.fullmatch(word) is not None)

    def test_a_key_that_is_not_text_is_refused_rather_than_raising(self):
        """`tomllib` hands a table or an array straight through, and
        `_HOTKEY_RE.fullmatch` raises `TypeError` for either — in a module every command
        imports, `charter --version` included. `density_level`'s own guard, one key over.
        """
        for value in ([], {}, ["F2"], 2, True, None, 1.5):
            with self.subTest(value=value):
                self.assertIsNone(instance.toggle_key(value))

    def test_a_usable_key_reaches_the_placement(self):
        """The control: without it every refusal below passes against a `key` nothing
        ever reads."""
        got = instance.frame_of(
            {"frame": {"component": [{"use": "identity", "key": "M-i"}]}})
        self.assertEqual([(p["slot"], p["key"]) for p in got["components"]],
                         [("top", "M-i")])

    def test_a_key_charter_will_not_bind_refuses_the_whole_arrangement(self):
        """#535's rule, applied to the new field: refused whole, so the operator sees
        their arrangement not take effect rather than one panel quietly un-toggleable.

        The fixture reaches exactly ONE guard — a single table, a key no other component
        claims and no `[frame] hotkey` equals — so a survivor here cannot be a neighbour
        catching it for a different reason."""
        for key in ("F2\nbind -n F3 kill-server", "F2 ", "F2;kill-server", "F2#{q:x}",
                    "F2'", 'F2"', "F2 x", "", ["F2"], {"k": "F2"}, 7, True):
            with self.subTest(key=key):
                frame = instance.frame_of(
                    {"frame": {"hotkey": "M-x",
                               "component": [{"use": "identity", "key": key}]}})
                self.assertEqual(frame["components"], [])
                self.assertEqual(frame["slots"], instance.FRAME_DEFAULTS["slots"])

    def test_two_components_may_not_claim_one_key(self):
        """Only the duplicate guard can fire here: `F5` is a key `_HOTKEY_RE` accepts and
        is not the frame's hotkey. tmux has no notion of a conflict — the later `bind -n`
        replaces the earlier — so unrefused this is one panel that cannot be toggled at
        all and nothing anywhere saying which."""
        frame = instance.frame_of({"frame": {"hotkey": "M-x", "component": [
            {"use": "identity", "key": "F5"}, {"use": "attention", "key": "F5"}]}})
        self.assertEqual(frame["components"], [])

    def test_two_components_with_different_keys_are_the_control(self):
        frame = instance.frame_of({"frame": {"hotkey": "M-x", "component": [
            {"use": "identity", "key": "F5"}, {"use": "attention", "key": "F6"}]}})
        self.assertEqual([p["key"] for p in frame["components"]], ["F5", "F6"])

    def test_a_component_may_not_take_the_frames_own_palette_key(self):
        """Only the collision guard can fire: `F2` is usable and no other component
        claims it. `conf_text` writes the palette's bind FIRST, so a later `bind -n F2`
        here replaces it — and since Task 4 deleted the menu (§4h), that is every action
        charter has: the density levels, both pickers, detach, all of it, gone from every
        frame on the socket with nothing left to open them with."""
        frame = instance.frame_of(
            {"frame": {"component": [{"use": "identity", "key": "F2"}]}})
        self.assertEqual(frame["components"], [])
        self.assertEqual(frame["slots"], instance.FRAME_DEFAULTS["slots"])

    def test_a_component_may_not_take_the_escape_hatchs_key(self):
        """Task 2 (#554) bound `F12` as the escape hatch — the one key that must keep
        working when charter's own code does not. A committed `key = "F12"` would be a
        second `bind -n` for it.

        **Refused here rather than left to the emission order, which happens to be
        harmless.** `conf_text` writes toggle binds BEFORE `overlay.hatch_bind()`, so
        tmux's last-wins would leave the hatch alive and the component's key silently
        dead — an operator's key doing nothing, with nothing saying why. Relying on that
        would also make this refusal unpinnable, since deleting it would change nothing
        observable: a guard whose consequence depends on where two other lines are emitted
        is the masked guard #553 was.

        Only this refusal can fire: `F12` is a key `_HOTKEY_RE` accepts, no other component
        claims it, and the frame's palette is on `M-x`."""
        frame = instance.frame_of({"frame": {"hotkey": "M-x", "component": [
            {"use": "identity", "key": overlay.HATCH_KEY}]}})
        self.assertEqual(frame["components"], [])
        self.assertEqual(frame["slots"], instance.FRAME_DEFAULTS["slots"])

    def test_a_key_charter_has_not_taken_is_the_control(self):
        """Without this, a `component_tables` that refused every key would pass the test
        above. `F11` is next to the hatch and is nobody's."""
        frame = instance.frame_of({"frame": {"hotkey": "M-x", "component": [
            {"use": "identity", "key": "F11"}]}})
        self.assertEqual([p["key"] for p in frame["components"]], ["F11"])

    def test_the_collision_is_with_the_key_actually_bound_not_the_shipped_one(self):
        """The operator moved their palette to `M-x`, so `F2` is theirs to bind and `M-x` is
        not. Asserted in both directions, because a guard comparing against the shipped
        `F2` constant instead of the RESOLVED hotkey passes the test above and is wrong
        on every plane that set the key."""
        frame = {"hotkey": "M-x", "component": [{"use": "identity", "key": "F2"}]}
        self.assertEqual([p["key"] for p in instance.frame_of({"frame": frame})
                          ["components"]], ["F2"])
        frame["component"] = [{"use": "identity", "key": "M-x"}]
        self.assertEqual(instance.frame_of({"frame": frame})["components"], [])

    def test_an_unusable_hotkey_collides_at_the_key_that_replaces_it(self):
        """A refused `[frame] hotkey` degrades to the shipped `F2` (`_HOTKEY_RE`), and
        `F2` is then what the palette is bound to — so `F2` is what a component may not take.
        The comparison is against `frame_of`'s resolved answer for that reason, not
        against what the file said."""
        frame = instance.frame_of(
            {"frame": {"hotkey": "not a key at all",
                       "component": [{"use": "identity", "key": "F2"}]}})
        self.assertEqual(frame["hotkey"], "F2")
        self.assertEqual(frame["components"], [])

    def test_the_arrangement_is_one_answer_however_it_is_asked(self):
        """`frame_components` used to resolve the committed tables a SECOND time, without
        the hotkey — so it would have accepted the very arrangement `config.FRAME` had
        already thrown away. Two answers to one question is what #547 cost."""
        cfg = {"frame": {"component": [{"use": "identity", "key": "F2"}]}}
        self.assertEqual(instance.frame_of(cfg)["components"], [])
        self.assertEqual([p["use"] for p in instance.frame_components(cfg)],
                         ["identity", "attention", "repos", "sidebar"])

    def test_an_arrangement_with_no_keys_at_all_still_resolves(self):
        """The overwhelmingly common case, and the control for every refusal above: a
        table with no `key` is not a table with a bad one."""
        frame = instance.frame_of(
            {"frame": {"component": [{"use": "identity"}, {"use": "attention"}]}})
        self.assertEqual([(p["slot"], p["key"]) for p in frame["components"]],
                         [("top", None), ("bottom", None)])
        self.assertEqual(instance.frame_toggles(frame), {})


class TheBindReachesTmuxConfigText(unittest.TestCase):
    """`conf_text` writes one `bind -n` per toggle — and refuses both halves of a line it
    cannot write safely.

    This is the `source-file` boundary: `instance._HOTKEY_RE`'s docstring records
    measuring, on tmux 3.7c, that a newline in a value interpolated here makes
    `source-file` return **0**, silently, and runs a second tmux command at launch with no
    keypress. So the assertions are about the TEXT, and the hostile values are run rather
    than reasoned about.
    """

    def _text(self, toggles):
        return commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                        session="fr-1", toggles=toggles)

    def test_a_key_binds_the_toggle_for_its_own_component(self):
        text = self._text({"repos": "F7"})
        self.assertIn("bind -n F7 run-shell "
                      "'\"$CHARTER_PY\" -m charter frame-toggle repos "
                      "--chat \"#{@charter_chat}\"'", text)

    def test_every_declared_toggle_gets_its_own_line_in_split_order(self):
        text = self._text({"top": "F5", "bottom": "F6", "repos": "F7"})
        bound = [ln.split()[2] for ln in text.split("\n") if "frame-toggle" in ln]
        self.assertEqual(bound, ["F5", "F6", "F7"])

    def test_a_plane_that_declared_none_gets_the_config_it_always_got(self):
        """The default is not a hidden feature: a `slots`-spelled plane has nowhere to
        write a key, so its frame's config must be byte-identical to the one it had."""
        self.assertEqual(self._text({}), self._text(None))
        self.assertNotIn("frame-toggle", self._text({}))

    def test_a_hostile_component_name_never_reaches_the_bind_line(self):
        """Only the name guard is in play — every key here is a plain `F7`. A survivor is
        a `bind` line ending early and a second tmux command starting."""
        for name in ("repos\nbind -n F3 kill-server", "repos'", 'repos"', "repos;x",
                     "repos #{q:x}", "repos x", "repos\x1b[31m", "../etc", "REPOS",
                     "", "repos bottom"):
            with self.subTest(name=name):
                text = self._text({name: "F7"})
                self.assertNotIn("frame-toggle", text, name)
                self.assertEqual(len(text.split("\n")), len(self._text({}).split("\n")))

    def test_a_hostile_key_never_reaches_the_bind_line(self):
        """Only the key guard is in play — every name here is a plain `repos`. The
        newline payload is `instance._HOTKEY_RE`'s own measured exploit, moved one key
        over."""
        for key in ("F7\nrun-shell 'touch /tmp/PWNED'", "F7 ", "F7;kill-server",
                    "F7#{q:x}", "F7'", 'F7"', "F7 x", "", None, ["F7"], 7, True):
            with self.subTest(key=key):
                text = self._text({"repos": key})
                self.assertNotIn("frame-toggle", text, repr(key))
                self.assertEqual(len(text.split("\n")), len(self._text({}).split("\n")))

    def test_a_refused_pair_costs_its_key_and_nothing_else(self):
        """The degrade, stated: the other keys still bind and the settings above them are
        untouched, because the alternative is a `source-file` that fails and takes
        `mouse`, `history-limit` and the palette's own hotkey down with it."""
        text = self._text({"top": "F5", "repos": "F7\nkill-server", "bottom": "F6"})
        bound = [ln.split()[2] for ln in text.split("\n") if "frame-toggle" in ln]
        self.assertEqual(bound, ["F5", "F6"])
        self.assertIn("set -t fr-1 status off", text)
        self.assertIn("bind -n F2 run-shell", text)

    def test_the_toggle_binds_are_written_after_the_palettes_own(self):
        """Order is what keeps `instance.component_tables`' collision refusal honest. tmux
        replaces a bind rather than reporting a conflict, so a component that reached this
        function with the palette's key would take the palette away — a real, observable
        consequence, which is what makes that refusal pinnable. Emitting these first would
        make the same deletion look harmless."""
        lines = self._text({"repos": "F7"}).split("\n")
        self.assertLess(next(i for i, ln in enumerate(lines) if "frame-palette" in ln),
                        next(i for i, ln in enumerate(lines) if "frame-toggle" in ln))

    def test_the_escape_hatch_is_still_the_last_line(self):
        """Task 2's invariant, which this change had to be inserted underneath rather than
        appended after. `frame/overlay.py`'s `hatch_bind` uses `run-shell -C`, which first
        parses in tmux 3.2 (`tmuxctl.FLOOR`), so it goes last precisely so that a tmux too
        old to parse it has already applied everything else in the file. A toggle bind
        appended after it would be the first thing such a tmux dropped — the operator's
        keys lost to a version skew that costs nothing else.

        Asserted with toggles present AND absent, because the interesting case is the one
        this task introduced and the other is the control."""
        for toggles in ({"repos": "F7", "right": "M-s"}, {}, None):
            with self.subTest(toggles=toggles):
                lines = [ln for ln in self._text(toggles).split("\n") if ln]
                self.assertEqual(lines[-1], overlay.hatch_bind())

    def test_a_toggle_bind_sits_between_the_palette_and_the_hatch(self):
        """The whole ordering, in one assertion, because the two ends are load-bearing for
        opposite reasons: after the palette so a colliding key would visibly steal it (which
        is what makes `component_tables`' refusal pinnable), before the hatch so a tmux
        below the floor never drops it."""
        lines = self._text({"repos": "F7"}).split("\n")
        palette = next(i for i, ln in enumerate(lines) if "frame-palette" in ln)
        toggle = next(i for i, ln in enumerate(lines) if "frame-toggle" in ln)
        hatch = next(i for i, ln in enumerate(lines) if ln == overlay.hatch_bind())
        self.assertLess(palette, toggle)
        self.assertLess(toggle, hatch)

    def test_the_interpreter_is_carried_out_of_band_like_every_other_bind(self):
        """`"$CHARTER_PY" -m charter`, never a bare `charter` — #390 and `conf_text`'s own
        docstring. A bare name resolves to whatever the tmux server's PATH has, and
        `run-shell` reports the 127 by printing it INTO THE HARNESS PANE."""
        line = next(ln for ln in self._text({"repos": "F7"}).split("\n")
                    if "frame-toggle" in ln)
        self.assertIn('"$CHARTER_PY" -m charter', line)
        self.assertNotIn("'charter ", line)


class DensityIsANamedArrangementOverVisibility(PersonaIso, unittest.TestCase):
    """A level names a set of visible components; it does not drive a layout of its own.

    The operator's own words for why this had to change: *"instead of having density - we
    need to have hotkeys to hide and show separately components"*. If a level kept its own
    path, a toggle afterwards would be a second idea about what the frame is, and the two
    would take turns overwriting each other.
    """

    def setUp(self):
        super().setUp()
        self.fid = f"dn-{_a_dead_pid()}"
        self.enterContext(mock.patch.dict(os.environ,
                                          {"CHARTER_SESSION_ID": self.fid}))
        self.enterContext(mock.patch("charter.frame.tmuxctl.version",
                                     return_value=(3, 7)))
        state.record_harness_pane(self.fid, "%0")
        state.record_panes(self.fid, panels={"top": "%1", "bottom": "%2",
                                             "repos": "%5", "right": "%4"})

    def _density(self, level, fake=None):
        fake = fake or _Tmux()
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            rc = commands_frame.cmd_density(SimpleNamespace(level=level))
        return rc, fake

    def _toggle(self, name):
        fake = _Tmux()
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            rc = commands_frame.cmd_toggle(SimpleNamespace(component=name))
        return rc, fake

    def test_a_level_is_recorded_as_the_components_it_hides(self):
        rc, _ = self._density("minimal")
        self.assertEqual(rc, 0)
        self.assertEqual(state.hidden(self.fid), ("repos", "right"))

    def test_the_level_itself_is_still_recorded_for_the_verbosity(self):
        """The one axis a per-component key cannot express: how much each panel SAYS. A
        level that only wrote a hidden set would silently drop `terse`."""
        self._density("minimal")
        self.assertEqual(state.density(self.fid), "minimal")

    def test_a_key_after_a_level_composes_with_it_instead_of_undoing_it(self):
        """The property the whole design turns on. `minimal` hides the table and the
        sidebar; the sidebar's own key then brings back the sidebar AND NOTHING ELSE."""
        self._density("minimal")
        rc, fake = self._toggle("right")
        self.assertEqual(rc, 0)
        self.assertEqual(state.hidden(self.fid), ("repos",))
        self.assertEqual(fake.split(), ["right"], fake.calls)

    def test_a_level_after_a_key_replaces_the_whole_set(self):
        """The other direction, and why a level is a NAME for an arrangement rather than
        one more edit to it: picking `full` means "everything", including whatever was
        toggled off before it."""
        self._toggle("repos")
        self.assertEqual(state.hidden(self.fid), ("repos",))
        self._density("full")
        self.assertEqual(state.hidden(self.fid), ())

    def test_the_level_names_a_set_and_the_arrangement_says_where_they_go(self):
        """Split order is the plane's, not the level's. `instance.FRAME_DENSITY`'s lists
        are in charter's shipped order because they had to name one; an operator's own
        `[frame] slots` order is a promise `instance.frame_of` keeps verbatim, and
        `layout.repos_cols`' whole docstring is about that order being the geometry.

        Asserted with NO panes recorded, which is the only shape that tells the two
        orders apart: with a pane already there it survives in place and everything else
        is split after it in the same relative order either way. From nothing, every pane
        is split in *want*'s order and the recorded map is that order."""
        frame = instance.frame_of({"frame": {"slots": ["right", "top", "bottom"]}})
        with mock.patch.dict(config.FRAME, frame):
            state.record_panes(self.fid, panels={})
            self._density("full", _Tmux(new_panes=("%7", "%8", "%9", "%10")))
        self.assertEqual(list(state.panes(self.fid)),
                         ["right", "top", "bottom", "repos"])

    def test_the_universe_is_the_planes_own_order_and_then_charters(self):
        """Asserted on a `slots` list that is NOT a prefix of the shipped one, which is
        the only shape that tells the two halves apart: reading the plane's own order
        first, and appending the built-ins it never named. `["right", "top"]` is the
        shortest list with that property — `tests/test_frame_config.py` uses it for the
        same reason."""
        frame = instance.frame_of({"frame": {"slots": ["right", "top"]}})
        self.assertEqual(instance.frame_arrangement(frame),
                         ["right", "top", "bottom", "repos"])

    def test_a_written_out_arrangement_keeps_its_hidden_panels_in_the_universe(self):
        """`visible = false` is a component that EXISTS and is off, which `slots` cannot
        say at all — so the universe has to come from the placements rather than from the
        visible list they resolve down to. Read the other way, `repos` would lose the
        position its own file gives it and come back at charter's."""
        frame = instance.frame_of({"frame": {"component": [
            {"use": "repos", "visible": False}, {"use": "identity"}]}})
        self.assertEqual(frame["slots"], ["top"])
        self.assertEqual(instance.frame_arrangement(frame),
                         ["repos", "top", "bottom", "right"])

    def test_a_level_can_still_name_a_panel_the_plane_never_listed(self):
        """`slots = ["top", "bottom"]` and `full` — behaviour every plane has had since
        presets existed, and the reason `frame_arrangement` appends charter's own
        built-ins to the universe rather than stopping at what the file wrote."""
        frame = instance.frame_of({"frame": {"slots": ["top", "bottom"]}})
        self.assertEqual(instance.frame_arrangement(frame),
                         ["top", "bottom", "repos", "right"])
        with mock.patch.dict(config.FRAME, frame):
            self._density("full")
        self.assertEqual(state.hidden(self.fid), ())
        self.assertEqual(sorted(state.panes(self.fid)),
                         ["bottom", "repos", "right", "top"])


class WhatTheFrameIsNotDrawingRightNow(PersonaIso, unittest.TestCase):
    """`state.hidden` and `commands_frame._hidden_now` — the two-source read that makes
    "for the running frame only" true."""

    def setUp(self):
        super().setUp()
        self.fid = f"hd-{_a_dead_pid()}"

    def test_a_frame_nobody_has_touched_has_recorded_nothing(self):
        self.assertIsNone(state.hidden(self.fid))

    def test_recorded_empty_and_never_recorded_are_different_answers(self):
        """The distinction `record_hidden` writes a newline-per-name file to keep. Without
        it, an operator who toggles the last hidden panel back on has that panel put
        straight back by the config on the next repaint, and the key looks broken."""
        state.record_hidden(self.fid, [])
        self.assertEqual(state.hidden(self.fid), ())
        self.assertIsNotNone(state.hidden(self.fid))

    def test_the_config_answers_with_everything_it_is_not_showing(self):
        """The arrangement minus the visible list, in arrangement order. `repos` is
        hidden because the file says `visible = false`; `attention` and `sidebar` are
        hidden because this plane never placed them at all, and they are only in the
        universe so a density level can still reach them."""
        frame = instance.frame_of({"frame": {"component": [
            {"use": "identity"}, {"use": "repos", "visible": False}]}})
        self.assertEqual(frame["slots"], ["top"])
        self.assertEqual(commands_frame._hidden_now(self.fid, frame),
                         ["repos", "bottom", "right"])

    def test_a_trimmed_slot_list_is_read_the_same_way(self):
        """The commonest plane there is, and the one a `visible = false`-only reading got
        wrong: `slots = ["top", "bottom"]` shows two panels, so the other two are hidden.
        Read as "nothing is hidden", the very first keypress on such a plane would have
        conjured the repo table and the sidebar out of nowhere."""
        frame = instance.frame_of({"frame": {"slots": ["top", "bottom"]}})
        self.assertEqual(commands_frame._hidden_now(self.fid, frame),
                         ["repos", "right"])

    def test_a_recorded_set_beats_the_configured_one(self):
        frame = instance.frame_of({"frame": {"component": [
            {"use": "identity"}, {"use": "repos", "visible": False}]}})
        state.record_hidden(self.fid, [])
        self.assertEqual(commands_frame._hidden_now(self.fid, frame), [])

    def test_it_round_trips_in_order(self):
        state.record_hidden(self.fid, ["repos", "right"])
        self.assertEqual(state.hidden(self.fid), ("repos", "right"))

    def test_a_frame_id_that_cannot_name_a_directory_writes_nothing_and_raises_nothing(self):
        """`$CHARTER_SESSION_ID` reaches both halves without ever going through
        `state.frame_id`'s minting, so a hostile or oversized id is a real input and
        `contain.child` answers ``None`` for it. Both run inside a `run-shell` child,
        where an exception is a traceback printed INTO THE HARNESS PANE — the one
        rectangle ADR 0018 says charter never draws in."""
        for fid in ("../evil", "a/b", "", ".", "x" * 5000):
            with self.subTest(fid=fid):
                state.record_hidden(fid, ["repos"])          # must not raise
                self.assertIsNone(state.hidden(fid))

    def test_a_state_directory_that_cannot_be_written_is_not_an_exception(self):
        """The other half of the same promise, one layer down: a full disk, a read-only
        state directory, an `os.replace` across a boundary. The keypress does nothing,
        which is what every other writer in `frame/state.py` already does.

        **Not `Path.write_text` any more, and the reason is the change that broke it.**
        This used to patch `pathlib.Path.write_text`, which pinned the test to the WRITER'S
        SPELLING rather than to the property it is named for: routing the writer through
        `config.write_for` (#582) left the mock aimed at a call nobody makes, the write
        succeeded, and the case went red having found nothing. `config.write_for` is what
        `frame/state.py` actually depends on, so that is what a failing filesystem is
        injected at — the same shape `test_frame_gather` uses for its unlink.
        """
        with mock.patch.object(state.config, "write_for",
                               side_effect=OSError("no space")):
            state.record_hidden(self.fid, ["repos"])         # must not raise
        self.assertIsNone(state.hidden(self.fid))

    def test_a_new_frame_never_inherits_a_dead_ones_hidden_set(self):
        """#383's recycled pid, one file over. A frame id is `<workspace>-<launcher pid>`,
        and a launcher landing on a pid an earlier launcher used adopts that directory —
        so a panel the previous operator dismissed would be missing from a brand-new
        frame, with `[frame] slots` naming it and nothing on screen to say why."""
        state.record_hidden(self.fid, ["repos"])
        state.clear_shape(self.fid)
        self.assertIsNone(state.hidden(self.fid))


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
