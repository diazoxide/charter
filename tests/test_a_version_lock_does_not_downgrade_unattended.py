"""`[charter] version` is committed data, and it replaces the binary that enforces the guard.

#333, from the authority audit of 0.47.2. `hooks._autosync_version_lock` conforms this
machine to the pin at SessionStart, and its own docstring says the stakes out loud: "this
replaces the binary that enforces the credential guard". `instance.locked_version` checked
the value for being a non-empty string and nothing else, so the pin reached
``uv tool install charter-cp==<value>`` unchanged.

**Two things were wrong, and they are separate.**

*Shape.* ``charter-cp==<value>`` is a PEP 440 requirement specifier, not a version slot —
the same mistake #332 found one field over in an npm package spec. Demonstrated against the
real resolver: ``uv pip compile`` turns ``charter-cp==0.*`` into ``charter-cp==0.47.2``. A
pin that reads as exact silently means "whatever is latest", and it also defeats any
comparison of the two versions, because it is not one.

*Direction.* The lock is exact and therefore bidirectional, which is deliberate — pinning a
fleet back to a known-good release is a real case. What nothing distinguished is that an
upgrade can only ADD guards while a downgrade can only remove them: a committed
``version = "0.47.1"`` reinstalls, on every teammate's next session, the build in which
#317 was open. So the two directions are now treated differently at the unattended site
only. A person who types `charter version sync --cli` still downgrades.

**Why "report, never install" rather than "ask" or "refuse below a floor".** SessionStart
has no `ask` verdict — the only thing it can emit is context, and the only reader is a
model, which is not the human whose consent a downgrade needs. A shipped floor was the
other candidate and is worse: it is a number that ages, it refuses a legitimate pin-back to
anything older than itself, and the version below it that an attacker picks is simply the
one whose own floor was lower. Reporting costs the legitimate case one deliberate command,
run by the person who read the message.

**Nothing here installs anything.** `commands.sync_to` is stubbed in every case that could
reach it, and the cases that check the argv call `_sync_cmd`, which builds a list and runs
nothing. A test that really installed a charter version would replace the binary running
the suite.

**Preconditions are asserted, not assumed.** Every "it did not install" case is paired with
the identical call under a benign value that DOES install, so a vacuous pass — a hook that
never fired, a lock never read — cannot be mistaken for a refusal.
"""

from __future__ import annotations

import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import mock

from charter import __version__, commands, config, hooks, instance
from tests._isolation import PersonaIso, PlaneIso, no_background_refresh, run_hook

#: A pin below whatever is installed. Derived, never a literal: a hardcoded "0.0.1" would
#: stop being a downgrade the day charter's own version dropped below it, which is absurd
#: but so is a guard that quietly stops testing what it claims to.
_OLDER = "0.0.1"
_NEWER = "999.0.0"

#: What a PEP 440 specifier accepts on the right of ``==`` that is not an exact version.
#: ``0.*`` is the one that matters and it is not hypothetical — `uv pip compile` resolves
#: ``charter-cp==0.*`` to the latest 0.x. The rest are refused for the same reason: the
#: slot is a version, and everything else in it means something charter did not choose.
_NOT_A_VERSION = (
    "0.*",              # prefix match — reads exact, resolves to latest
    "0.47.*",
    ">=0.47.0",         # a range: the pin stops pinning
    "latest",           # a name, not a version
    "0.47.2 ; python_version < '4'",   # an environment marker rides along
    "0.47.2[extra]",
    "0.47.2-CANARY",    # a prefix match reads this as 0.47.2, and installs something else
    "0.47",             # not the three-part shape every charter release has had
    "v0.47.2",          # the tag, not the version it carries
    "0.47.2 --index-url",
)


class LockShape(unittest.TestCase):
    """The pin is a version before it is a requirement specifier."""

    def test_every_real_charter_version_is_accepted(self):
        """The benign path first: this gate must not refuse what charter publishes."""
        self.assertTrue(instance.version_ok(__version__))
        for v in ("0.1.0", "0.47.2", "1.0.0", "10.20.30"):
            self.assertTrue(instance.version_ok(v), v)

    def test_a_specifier_that_is_not_a_version_is_refused(self):
        for v in _NOT_A_VERSION:
            self.assertFalse(instance.version_ok(v), v)

    def test_the_refusal_names_the_value(self):
        """A reader who hits this has a defect in a committed file, not a typo."""
        self.assertIn("0.*", instance.NOT_A_VERSION.format(version="0.*"))

    def test_locked_version_still_reports_the_raw_pin(self):
        """Refusing at the READ would make a bad pin indistinguishable from no pin —
        the plane would silently behave as unpinned. The value comes back; the acting
        sites are what refuse it."""
        with TemporaryDirectory() as td:
            root = Path(td)
            (root / "charter.toml").write_text('schema = 1\n\n[charter]\nversion = "0.*"\n')
            self.assertEqual(instance.locked_version(instance.load(root)), "0.*")


class TheInstallerIsTheLastGate(unittest.TestCase):
    """`sync_to` answers to the same rule, wherever the version came from."""

    def test_the_benign_path_still_builds_the_same_argv(self):
        cmd = commands._sync_cmd("1.2.3")
        self.assertIn("charter-cp==1.2.3", cmd)
        self.assertIn("--force", cmd)
        self.assertIn("--refresh", cmd)

    def test_a_non_version_never_reaches_an_argv(self):
        for v in _NOT_A_VERSION:
            with self.assertRaises(ValueError, msg=v):
                commands._sync_cmd(v)

    def test_sync_to_refuses_without_running_anything(self):
        ran: list = []
        real = commands.util.run
        commands.util.run = lambda *a, **k: ran.append(a) or None
        self.addCleanup(lambda: setattr(commands.util, "run", real))
        ok, detail = commands.sync_to("0.*")
        self.assertFalse(ok)
        self.assertIn("0.*", detail)
        self.assertEqual(ran, [], "the refused value reached a subprocess")


class SessionStartConformance(PlaneIso):
    """Driven through the real hook entry point, with the installer stubbed."""

    def setUp(self) -> None:
        super().setUp()
        # The hook starts the newer-charter check since #938, and a fresh plane has no
        # cache or cooldown lock to stop the fork. This class is about the pin.
        no_background_refresh(self)
        self.root = config.ROOT
        self.installs: list[str] = []
        self._sync = commands.sync_to
        commands.sync_to = lambda v: (self.installs.append(v), (True, v))[1]
        self.addCleanup(lambda: setattr(commands, "sync_to", self._sync))

    def _lock(self, version: str) -> None:
        (self.root / "charter.toml").write_text(
            f'schema = 1\n\n[charter]\nversion = "{version}"\n')

    def _context(self) -> str:
        out = run_hook(hooks.sessionstart, {"session_id": "s", "cwd": str(self.root)})
        self.assertIsNotNone(out, "the SessionStart hook emitted nothing at all")
        return out["hookSpecificOutput"]["additionalContext"]

    # -- the precondition: the lock is live on this path ------------------- #

    def test_an_upgrade_still_installs_unattended(self):
        """PRECONDITION for every refusal below. If this stops installing, the cases
        that assert `installs == []` are passing for the wrong reason."""
        self._lock(_NEWER)
        ctx = self._context()
        self.assertEqual(self.installs, [_NEWER])
        self.assertIn(_NEWER, ctx)

    # -- direction ---------------------------------------------------------- #

    def test_a_downgrade_installs_nothing(self):
        self._lock(_OLDER)
        ctx = self._context()
        self.assertEqual(self.installs, [], "a downgrade was installed unattended")
        self.assertIn(_OLDER, ctx, "the refusal did not name the pin")

    def test_the_downgrade_message_says_what_to_run_instead(self):
        """A refusal that does not name the deliberate path is just an obstacle."""
        self._lock(_OLDER)
        ctx = self._context()
        self.assertIn("charter version sync", ctx)
        self.assertIn(__version__, ctx, "the message did not say what is running")

    def test_the_downgrade_message_says_why(self):
        """The reader has to be able to tell a refusal from a failure."""
        self._lock(_OLDER)
        low = self._context().lower()
        self.assertIn("older", low)
        self.assertNotIn("auto-updated", low)

    def test_an_attended_sync_still_downgrades(self):
        """The gate is on the UNATTENDED site. `charter version sync --cli` is a person
        typing, and pinning a fleet back to a known-good release must stay possible."""
        self._lock(_OLDER)
        rc = commands.cmd_version_sync(type("A", (), {"cli": True})())
        self.assertEqual(self.installs, [_OLDER])
        self.assertEqual(rc, 0)

    # -- a suffix on the running version (#1050) ---------------------------- #

    def test_a_candidate_running_still_upgrades_to_the_pinned_release(self):
        """PRECONDITION for the case below, on the same pin: a suffix alone is not what
        refuses. The release is newer than its candidate, so it installs."""
        self._lock("5.0.0")
        with mock.patch("charter.__version__", "5.0.0rc1"):
            self._context()
        self.assertEqual(self.installs, ["5.0.0"])

    def test_a_post_release_running_is_not_downgraded_to_the_release_it_follows(self):
        """`_parse_version` matched the ``X.Y.Z`` prefix and dropped the rest, so a running
        ``5.0.0.post1`` and a pin on ``5.0.0`` were one version, and not being older, the
        pin was installed: the downgrade this class refuses, let through by a suffix."""
        self._lock("5.0.0")
        with mock.patch("charter.__version__", "5.0.0.post1"):
            ctx = self._context()
        self.assertEqual(self.installs, [], "a downgrade was installed unattended")
        self.assertIn("older", ctx.lower())

    # -- the same release, spelled another way (#1050) ----------------------- #

    def test_a_pin_on_the_running_release_spelled_another_way_installs_nothing(self):
        """`instance.version_ok` takes ``5.0.00``, three runs of digits, and PEP 440 reads
        it as ``5.0.0``. It is not the running version's STRING, so the equality above lets
        it by, and it is not older, so it was installed: the running release reinstalled at
        every session start and announced as "auto-updated". It is the release running, so
        nothing is installed and nothing is said. Called directly, because silence is the
        answer and `_context` asserts the hook said something."""
        self._lock("5.0.00")
        with mock.patch("charter.__version__", "5.0.0"):
            said = hooks._autosync_version_lock()
        self.assertEqual(self.installs, [], "the running release was reinstalled")
        self.assertIsNone(said)

    def test_a_running_build_labelled_as_its_release_is_on_a_pin_of_that_release(self):
        """A local label names where a build came from, not a later release, so a pin on
        the release it labels is neither a move nor a downgrade."""
        self._lock("5.0.0")
        with mock.patch("charter.__version__", "5.0.0+dev"):
            said = hooks._autosync_version_lock()
        self.assertEqual(self.installs, [], "the running release was reinstalled")
        self.assertIsNone(said)

    def test_a_two_part_pin_is_refused_on_shape_before_any_order_is_asked(self):
        """``5.0`` IS ``5.0.0`` to PEP 440, and still not a pin: `version_ok` wants three
        parts, and says so, rather than the order calling it the running release."""
        self._lock("5.0")
        with mock.patch("charter.__version__", "5.0.0"):
            said = hooks._autosync_version_lock()
        self.assertEqual(self.installs, [])
        self.assertIn("is not a version", said)
        self.assertNotIn("older", said.lower())

    # -- shape -------------------------------------------------------------- #

    def test_a_pin_that_is_not_a_version_installs_nothing(self):
        for v in ("0.*", "latest", ">=0.1"):
            self.installs.clear()
            self._lock(v)
            ctx = self._context()
            self.assertEqual(self.installs, [], f"{v} reached the installer")
            self.assertIn(v, ctx, f"the refusal did not name {v}")

    def test_a_wildcard_pin_is_refused_rather_than_treated_as_a_downgrade(self):
        """`0.*` is not orderable, so the direction check cannot speak for it — it has
        to be refused on shape, or it falls through whichever way the comparison guesses."""
        self._lock("0.*")
        ctx = self._context()
        self.assertEqual(self.installs, [])
        self.assertNotIn("older", ctx.lower())

    # -- unchanged behaviour ------------------------------------------------ #

    def test_no_lock_still_does_nothing(self):
        (self.root / "charter.toml").write_text("schema = 1\n")
        self._context()
        self.assertEqual(self.installs, [])

    def test_a_matching_lock_still_does_nothing(self):
        self._lock(__version__)
        self.assertEqual(self.installs, [])

    def test_a_broken_control_plane_never_raises(self):
        (self.root / "charter.toml").write_text("this is not toml {{{")
        self.assertIsNone(hooks._autosync_version_lock())

    def test_a_failed_install_still_warns_and_never_raises(self):
        commands.sync_to = lambda v: (False, "offline")
        self._lock(_NEWER)
        msg = hooks._autosync_version_lock()
        self.assertIn("failed", msg.lower())


if __name__ == "__main__":
    unittest.main()
