"""The read half of the suite-wide tripwire: `[update] channel` is not a fixture (#459).

`charter update` on a charter checkout only refreshes the plugin when the plane is on the
dev channel, so a charter developer has to put ``[update] channel = "dev"`` in their own
`charter.toml` for their own tooling to work. That file is committed, and `charter save`
commits it. The first time it landed, six tests in two modules went red — not because the
change was wrong, but because `test_statusline_brand.UpdateIndicator` and
`test_version_lock.AutoSync` had never isolated `config.UPDATE` and had been quietly
reading whatever channel the machine declared. The feature was unusable until this closed.

Fixing those two classes was the smaller half. The guard in `tests/_planeguard.py` is the
other: it refuses the READ, so the next fixture to forget fails on the line that forgot,
with its own name in the message, instead of waiting for a contributor to opt their plane
in and discover it in CI. It found eight more sites the day it was written.

**Every case here is a control.** A guard nobody has watched fail is a guard nobody knows
works: `TheGuardIsNotBlind` makes the refusal happen for real, and the cases after it prove
each way OUT of the refusal actually works, so "green" cannot mean "the tripwire is gone".
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from charter import channel, config
from tests import _planeguard
from tests._isolation import PersonaIso, pin_update_channel


class WhatIsGuarded(unittest.TestCase):
    def test_the_channel_is_on_the_guarded_list(self):
        self.assertIn("UPDATE", _planeguard._GUARDED_SETTINGS)

    def test_every_guarded_name_is_a_setting_config_actually_derives(self):
        """A typo here would guard nothing and say nothing — the shape of a silent hole."""
        for name in _planeguard._GUARDED_SETTINGS:
            with self.subTest(setting=name):
                self.assertIn(name, config.DERIVED)

    def test_every_guarded_name_actually_derives_to_a_dict(self):
        """The name existing is not enough. Both arming sites skip a value that is not a
        `dict` — the refusal works by substituting an object that will not answer, and a
        `str` or a `Path` cannot be made to refuse without breaking every message that
        legitimately quotes it. So a future entry deriving to one of those would guard
        nothing while the check above stayed green: the silent hole its own docstring
        warns about, one level in."""
        with TemporaryDirectory() as tmp:
            derived = config.derive(Path(tmp))       # a tmp root, so nothing is armed
            for name in _planeguard._GUARDED_SETTINGS:
                with self.subTest(setting=name):
                    self.assertIsInstance(
                        derived[name], dict,
                        f"config.{name} does not derive to a dict, so _RefusesToBeRead "
                        f"cannot stand in for it and the guard silently skips it")

    def test_derive_is_wrapped_at_package_import(self):
        """Arming happens per derivation, not once at import: a `config.use(config.ROOT)`
        anywhere in the suite lands back on the real plane and has to re-arm."""
        self.assertEqual(getattr(config.derive, "__module__", None), "tests._planeguard")

    def test_the_refusal_is_not_an_exception(self):
        """`statusline._dev_chip` wraps `channel.is_dev()` in `except Exception` so a
        broken plane cannot cost the status line. A tripwire that catchable would be
        reported as "not on the dev channel" and the test would pass, wrongly."""
        self.assertTrue(issubclass(_planeguard.RealPlaneRead, BaseException))
        self.assertFalse(issubclass(_planeguard.RealPlaneRead, Exception))


class TheGuardIsNotBlind(unittest.TestCase):
    """A plain `TestCase`: `charter.config` still points at the developer's real plane,
    which is exactly the state every case below is about."""

    def test_reading_the_real_planes_channel_is_refused(self):
        with self.assertRaises(_planeguard.RealPlaneRead):
            channel.is_dev()

    def test_the_message_names_the_test_that_made_the_read(self):
        """`unittest` puts the test's name in the failure header; the message repeats it
        so an excerpt quoted into an issue or a CI log still says which test it was."""
        with self.assertRaises(_planeguard.RealPlaneRead) as caught:
            channel.is_dev()
        self.assertIn("test_the_message_names_the_test_that_made_the_read",
                      str(caught.exception))
        self.assertIn("UPDATE", str(caught.exception))
        self.assertIn("PersonaIso", str(caught.exception))

    def test_every_way_in_is_closed_not_just_the_one_channel_uses(self):
        """`channel.channel` reads it with `.get`, but a future reader may not.

        Two groups are worth naming. The C-level fast paths — `dict(x)`, `{**x}`, `|` —
        bypass an overridden `keys()` unless `__iter__` is overridden too. And the
        mutating accessors `pop`/`popitem`/`setdefault`, plus `__reversed__`, which each
        handed back the operator's real value until they were probed for: no reader in
        `charter/` uses any of them, which is exactly why they would have stayed open
        until one did. This list came from probing every entry point, not from reasoning
        about which ones matter.
        """
        guarded = config.UPDATE
        for label, read in (("get", lambda: guarded.get("channel")),
                            ("get w/ default", lambda: guarded.get("channel", "stable")),
                            ("[]", lambda: guarded["channel"]),
                            ("in", lambda: "channel" in guarded),
                            ("iter", lambda: list(guarded)),
                            ("reversed", lambda: list(reversed(guarded))),
                            ("keys", lambda: list(guarded.keys())),
                            ("items", lambda: list(guarded.items())),
                            ("values", lambda: list(guarded.values())),
                            ("copy", lambda: guarded.copy()),
                            ("pop", lambda: guarded.pop("channel")),
                            ("pop w/ default", lambda: guarded.pop("channel", None)),
                            ("popitem", lambda: guarded.popitem()),
                            ("setdefault", lambda: guarded.setdefault("channel", "x")),
                            ("dict()", lambda: dict(guarded)),
                            ("**", lambda: {**guarded}),
                            ("|", lambda: guarded | {"x": 1}),
                            ("r|", lambda: {"x": 1} | guarded),
                            ("json.dumps", lambda: json.dumps(guarded)),
                            ("format_map", lambda: "{channel}".format_map(guarded)),
                            ("==", lambda: guarded == {"channel": "stable"})):
            with self.subTest(read=label):
                with self.assertRaises(_planeguard.RealPlaneRead):
                    read()

    def test_a_refused_mutator_did_not_mutate_on_the_way_out(self):
        """`pop`/`popitem`/`setdefault` are refused BEFORE they touch anything, the way the
        write guard refuses `rmtree` at its front door. A tripwire that raised after the
        mutation would leave the next test reading a value this one silently edited."""
        before = dict.__len__(config.UPDATE)
        for attempt in (lambda: config.UPDATE.pop("channel"),
                        lambda: config.UPDATE.popitem(),
                        lambda: config.UPDATE.setdefault("channel", "tampered")):
            with self.assertRaises(_planeguard.RealPlaneRead):
                attempt()
        self.assertEqual(dict.__len__(config.UPDATE), before)
        self.assertEqual(dict.__repr__(config.UPDATE), repr(config.UPDATE))

    def test_what_is_deliberately_left_open_is_left_open(self):
        """`repr` and `len` (so `bool`) still answer, and that is the choice, not an
        oversight: `unittest`'s own error formatting, a traceback and a debugger all print
        this object, and a guard that raises while a test is already failing hides the
        failure it exists to explain. Neither one answers "which channel", which is the
        fact being protected. Pinned so a later tightening has to be deliberate."""
        self.assertEqual(len(config.UPDATE), 1)
        self.assertIn("channel", repr(config.UPDATE))
        self.assertTrue(bool(config.UPDATE))


class EveryWayOutOfTheRefusalWorks(unittest.TestCase):
    """The other direction. A guard with no usable exit gets deleted by whoever hits it
    next, so each documented escape is exercised here rather than described."""

    def test_a_tmp_root_derives_a_readable_value(self):
        with TemporaryDirectory() as tmp:
            derived = config.derive(Path(tmp))
            self.assertEqual(derived["UPDATE"], {"channel": "stable"})

    def test_a_tmp_root_that_declares_dev_reads_back_dev(self):
        """Isolation is not "the channel is always stable" — it is "the channel is what
        the FIXTURE says". A case about the dev channel writes it and gets it."""
        with TemporaryDirectory() as tmp:
            (Path(tmp) / "charter.toml").write_text(
                'schema = 1\n\n[update]\nchannel = "dev"\n')
            self.assertEqual(config.derive(Path(tmp))["UPDATE"], {"channel": "dev"})

    def test_deriving_the_real_root_again_re_arms(self):
        """The property that makes the guard survive a suite: `config.use(config.ROOT)`
        is a real thing two modules here do, and it must not disarm the tripwire."""
        again = config.derive(Path(_planeguard._REAL_ROOT[0]))["UPDATE"]
        self.assertIsInstance(again, _planeguard._RefusesToBeRead)

    def test_pin_update_channel_makes_the_read_answer(self):
        pin_update_channel(self)
        self.assertIs(channel.is_dev(), False)

    def test_pin_update_channel_can_pin_dev(self):
        pin_update_channel(self, "dev")
        self.assertIs(channel.is_dev(), True)


class IsolationIsTheOtherWayOut(PersonaIso):
    def test_persona_iso_derives_the_channel_from_its_own_tmp_plane(self):
        self.assertIs(channel.is_dev(), False)
        self.assertEqual(config.UPDATE, {"channel": "stable"})

    def test_a_persona_iso_fixture_can_declare_the_dev_channel(self):
        (self.tmp / "charter.toml").write_text('schema = 1\n\n[update]\nchannel = "dev"\n')
        config.use(self.tmp)
        self.assertIs(channel.is_dev(), True)


class TheGuardIsRestoredAfterIsolation(unittest.TestCase):
    """The failure mode a value-based tripwire has and this one must not: a case that
    isolates leaves the guard disarmed for everything that runs after it.

    Not hypothetical. `test_secret_exec.SecretExecMode` shadowed `PersonaIso`'s cleanup
    with a method of its own name, so neither the config restore nor the `rmtree` ran:
    `config.ROOT` was left on a tmp directory that still exists and holds no
    `charter.toml`, which makes every setting answer its default rather than raise. The
    1186 tests that ran after it read that non-plane, which is also why #459's own six
    failures did not reproduce in a full-suite run — `stable` is the default they assumed.
    `PersonaIso`'s cleanup is name-mangled now, and this is what notices if that regresses.
    """

    def test_a_completed_persona_iso_case_leaves_the_guard_armed(self):
        case = IsolationIsTheOtherWayOut(
            "test_persona_iso_derives_the_channel_from_its_own_tmp_plane")
        result = unittest.TestResult()
        case.run(result)
        self.assertTrue(result.wasSuccessful(), result.errors or result.failures)
        self.assertIsInstance(config.UPDATE, _planeguard._RefusesToBeRead)
        with self.assertRaises(_planeguard.RealPlaneRead):
            channel.is_dev()


if __name__ == "__main__":
    unittest.main()
