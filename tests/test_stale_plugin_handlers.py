"""A plugin BEHIND the CLI leaves handlers undispatched, and nothing said so (#306).

`skew_message` guards one direction only — a plugin NEWER than the CLI, which hard-fails
because the manifest dispatches `charter hook <name>` for a handler the CLI does not have.
The other direction fails softly and is the one that happens by default: `uv tool install
charter-cp --force` moves the CLI and touches no plugin.

`hooks/hooks.json` is what decides which handlers run at all, so an older manifest silently
runs fewer of them. Observed: a plane on plugin 0.44.1 with CLI 0.46.3 recorded 299 `ask`
events and 0 `ask-approved` — not because nothing was approved, but because
`posttooluse-bash`, the handler that records approvals, was never dispatched. The tally read
exactly like a guard that runs and finds nothing.

**Handler sets, not version numbers.** A patch behind that adds no handler changes nothing
about dispatch and must stay silent, or the row trains people to scroll past it. Comparing
what the manifest actually invokes against what the CLI actually ships removes that
judgement call entirely, and measures the thing that breaks instead of a proxy for it.
"""
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import hooks


def _plugin(version: str, handlers: list[str]) -> Path:
    root = Path(tempfile.mkdtemp(prefix="charter-plugin-")).resolve()
    (root / ".claude-plugin").mkdir(parents=True)
    (root / ".claude-plugin" / "plugin.json").write_text(
        json.dumps({"name": "charter", "version": version}))
    (root / ".claude-plugin" / "marketplace.json").write_text(
        json.dumps({"name": "charter"}))
    (root / "hooks").mkdir()
    (root / "hooks" / "hooks.json").write_text(json.dumps({"hooks": {"PostToolUse": [
        {"hooks": [{"type": "command",
                    "command": f"charter hook {h} --plugin-version {version}"}
                   for h in handlers]}]}}))
    return root


class TestWhatTheManifestDispatches(unittest.TestCase):
    def test_it_reads_the_handler_names_out_of_the_manifest(self):
        root = _plugin("0.44.1", ["sessionstart", "posttooluse"])
        self.assertEqual(hooks.dispatched_handlers(root),
                         {"sessionstart", "posttooluse"})

    def test_an_unreadable_manifest_is_not_an_answer(self):
        """None, not an empty set. "I could not look" must not render as "it dispatches
        nothing", which would report every handler in the CLI as missing."""
        root = Path(tempfile.mkdtemp(prefix="charter-noplugin-"))
        self.assertIsNone(hooks.dispatched_handlers(root))

    def test_it_ignores_commands_that_are_not_hook_dispatches(self):
        """The manifest also runs `charter workspace _autosave`, `charter doctor` and
        friends. Only `charter hook <name>` decides which handler runs."""
        root = _plugin("0.44.1", ["sessionstart"])
        p = root / "hooks" / "hooks.json"
        doc = json.loads(p.read_text())
        doc["hooks"]["SessionStart"] = [{"hooks": [
            {"type": "command", "command": "charter workspace _autosave >/dev/null 2>&1"}]}]
        p.write_text(json.dumps(doc))
        self.assertEqual(hooks.dispatched_handlers(root), {"sessionstart"})


class TestTheMessage(unittest.TestCase):
    def test_it_names_the_handlers_that_are_not_running(self):
        """A version pair is not actionable; "posttooluse-bash is not being dispatched
        here" is. Same standard `guardseen` sets by recording the harness and the age
        rather than a boolean."""
        root = _plugin("0.44.1", [h for h in hooks._HANDLERS if h != "posttooluse-bash"])
        msg = hooks.stale_plugin_message(root)
        self.assertIsNotNone(msg)
        self.assertIn("posttooluse-bash", msg)

    def test_a_plugin_that_dispatches_everything_is_silent(self):
        """The property that makes this version-independent: a patch behind adding no
        handler produces an empty diff, so nothing is said and the row keeps its meaning."""
        root = _plugin("0.0.1", list(hooks._HANDLERS))
        self.assertIsNone(hooks.stale_plugin_message(root))

    def test_the_shipped_manifest_says_nothing_against_its_own_cli(self):
        """The repo's own plugin must be in sync — this is the test that fails when a
        handler is added to `_HANDLERS` and not wired into `hooks/hooks.json`."""
        self.assertIsNone(hooks.stale_plugin_message(Path(__file__).resolve().parents[1]))

    def test_an_extra_handler_the_cli_lacks_is_not_this_message(self):
        """That is the NEWER-plugin case, and `skew_message` already hard-fails on it.
        Reporting it twice, in two voices, with two different remedies, is worse than
        either."""
        root = _plugin("9.9.9", list(hooks._HANDLERS) + ["posttooluse-fromthefuture"])
        self.assertIsNone(hooks.stale_plugin_message(root))

    def test_an_unreadable_manifest_says_nothing(self):
        self.assertIsNone(hooks.stale_plugin_message(
            Path(tempfile.mkdtemp(prefix="charter-noplugin-"))))

    def test_it_names_the_refresh_step_that_is_otherwise_a_no_op(self):
        """`plugin update` alone finds the marketplace clone advertising what it last
        fetched and correctly does nothing — the trap already recorded in doctor."""
        root = _plugin("0.44.1", ["sessionstart"])
        self.assertIn("marketplace update", hooks.stale_plugin_message(root))


class TestItReachesTheSession(unittest.TestCase):
    """The reason this is not simply a doctor WARN. `cmd_doctor` exits non-zero only on
    FAIL, and that exit code is the whole trigger for the SessionStart wrapper
    (`out=$(charter doctor) || printf …`) printing anything — so a WARN reaches nobody.
    charter already wrote that down, in the branch above the one this fixes.

    `systemMessage` is the channel that works: it renders at exit 0 and blocks nothing, and
    the newer-plugin case already uses it.
    """

    def setUp(self):
        hooks._pending_system.clear()
        self.addCleanup(hooks._pending_system.clear)

    def test_sessionstart_queues_it(self):
        root = _plugin("0.44.1", ["sessionstart"])
        with mock.patch.dict("os.environ", {"CLAUDE_PLUGIN_ROOT": str(root)}):
            hooks._queue_plugin_notices("sessionstart", "0.44.1")
        self.assertTrue(any("posttooluse-bash" in m for m in hooks._pending_system))

    def test_other_hooks_do_not(self):
        """`pretooluse` fires on every Bash call. Emitting there would print the same
        warning dozens of times a session and teach people to scroll past it — the gate the
        newer-plugin path already applies."""
        root = _plugin("0.44.1", ["sessionstart"])
        with mock.patch.dict("os.environ", {"CLAUDE_PLUGIN_ROOT": str(root)}):
            hooks._queue_plugin_notices("pretooluse", "0.44.1")
        self.assertEqual(hooks._pending_system, [])

    def test_nothing_is_queued_outside_the_plugin(self):
        """A bare `charter` from a terminal has no plugin to be behind."""
        with mock.patch.dict("os.environ", {}, clear=True):
            hooks._queue_plugin_notices("sessionstart", None)
        self.assertEqual(hooks._pending_system, [])


class TestDoctorAgrees(unittest.TestCase):
    """`doctor` already named the version pair and the upgrade steps for an older plugin —
    the issue's claim that it was silent does not hold. What it could not say was WHICH
    handlers were not running, and it said all of it at OK, where a green tick is what gets
    scanned.

    WARN, not FAIL: nothing is broken and `cmd_doctor` exits non-zero only on FAIL, which
    would turn a benign lag into a preflight failure. The reaching is the `systemMessage`'s
    job; this row is for the developer who asks directly.
    """

    def _check(self, root):
        from charter import doctor
        with mock.patch.dict("os.environ", {"CLAUDE_PLUGIN_ROOT": str(root)}):
            return doctor.check_plugin_skew()

    def test_missing_handlers_warn_and_are_named(self):
        from charter import doctor
        root = _plugin("0.44.1", [h for h in hooks._HANDLERS if h != "posttooluse-bash"])
        r = self._check(root)
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("posttooluse-bash", r.detail + (r.hint or ""))

    def test_an_older_plugin_that_dispatches_everything_stays_ok(self):
        """Version lag on its own is not a finding. Warning on it would train people to
        ignore the row, which costs the case that matters."""
        from charter import doctor
        root = _plugin("0.0.1", list(hooks._HANDLERS))
        self.assertEqual(self._check(root).status, doctor.OK)


if __name__ == "__main__":
    unittest.main()
