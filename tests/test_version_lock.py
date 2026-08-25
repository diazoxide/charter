"""`[charter] version` — the opt-in version lock, and the auto-sync that honours it.

Shared like a lockfile: committed, exact (so a pin-back to a known-good release is
expressible), and opt-in. A control plane that pins nothing keeps today's behaviour
exactly.

**Only the UPGRADE direction is applied unattended** — the pin is committed data, and
`tests/test_a_version_lock_does_not_downgrade_unattended.py` (#333) owns that rule and
the argument for it. This file keeps the parts that direction does not touch.

**No test here installs anything.** `commands.sync_to` is stubbed throughout; a
suite that reinstalls the CLI it is testing would be both slow and destructive —
during development this feature really did downgrade the running charter.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from tempfile import TemporaryDirectory

from charter import commands, config, hooks, instance
from tests._isolation import PersonaIso


class LockFile(unittest.TestCase):
    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.root = Path(self._td.name)
        self.addCleanup(self._td.cleanup)

    def _toml(self, body: str) -> None:
        (self.root / "charter.toml").write_text(body)

    def test_absent_lock_reads_as_none(self):
        self._toml("schema = 1\n")
        self.assertIsNone(instance.locked_version(instance.load(self.root)))

    def test_empty_lock_reads_as_none(self):
        self._toml('schema = 1\n\n[charter]\nversion = ""\n')
        self.assertIsNone(instance.locked_version(instance.load(self.root)))

    def test_set_creates_the_section_when_missing(self):
        self._toml("schema = 1\n")
        self.assertTrue(instance.set_locked_version(self.root, "0.7.1"))
        self.assertEqual(instance.locked_version(instance.load(self.root)), "0.7.1")

    def test_set_replaces_an_existing_lock(self):
        self._toml('schema = 1\n\n[charter]\nversion = "0.1.0"\n')
        instance.set_locked_version(self.root, "0.9.9")
        self.assertEqual(instance.locked_version(instance.load(self.root)), "0.9.9")

    def test_set_adds_the_key_to_an_existing_empty_section(self):
        self._toml("schema = 1\n\n[charter]\n")
        instance.set_locked_version(self.root, "0.7.1")
        self.assertEqual(instance.locked_version(instance.load(self.root)), "0.7.1")

    def test_comments_and_other_keys_survive(self):
        self._toml('# keep me\nschema = 1\n\n[workspace]\ndefault = "x"\n')
        instance.set_locked_version(self.root, "0.7.1")
        text = (self.root / "charter.toml").read_text()
        self.assertIn("# keep me", text)
        self.assertIn('default = "x"', text)

    def test_a_version_key_in_another_section_is_never_touched(self):
        """Regression: the first draft rewrote the first `version =` line in the file,
        which happily clobbered `[[forge]] version = "api-v4"` and left the lock alone
        — silent corruption of a committed config."""
        self._toml('schema = 1\n\n[[forge]]\nkind = "gitlab"\nversion = "api-v4"\n'
                   '\n[charter]\nversion = "0.1.0"\n')
        instance.set_locked_version(self.root, "0.9.9")
        text = (self.root / "charter.toml").read_text()
        self.assertIn('version = "api-v4"', text)
        self.assertEqual(instance.locked_version(instance.load(self.root)), "0.9.9")

    def test_set_on_an_unwritable_root_reports_failure(self):
        self.assertFalse(instance.set_locked_version(self.root / "nope", "0.7.1"))

    def test_the_written_file_is_still_valid_toml(self):
        self._toml('schema = 1\n\n[[forge]]\nkind = "github"\n')
        instance.set_locked_version(self.root, "0.7.1")
        instance.load(self.root)      # raises if the edit broke the syntax


class AutoSync(PersonaIso):
    """SessionStart conformance. Loud, non-blocking, and opt-in.

    `PersonaIso`, not ``config.ROOT = tmp``. Assigning that one attribute moves where
    `charter.toml` is READ from without re-deriving anything that follows from it, so
    `config.UPDATE` kept naming the channel of the plane the suite resolved. Every case
    below assumes the default `stable`: on the dev channel `hooks._autosync_version_lock`
    answers with the pin-versus-dev CONFLICT message instead of installing, so on a machine
    whose own `charter.toml` declares ``channel = "dev"`` these went red and nowhere else
    (#459). `config.use` derives the whole set from the fixture root in one call.
    """

    def setUp(self) -> None:
        super().setUp()
        self.root = self.tmp
        self.installs: list[str] = []
        self._sync = commands.sync_to
        commands.sync_to = lambda v: (self.installs.append(v), (True, v))[1]
        self.addCleanup(lambda: setattr(commands, "sync_to", self._sync))

    def _lock(self, version: str | None) -> None:
        (self.root / "charter.toml").write_text("schema = 1\n")
        if version:
            instance.set_locked_version(self.root, version)
        # Re-derive: the fixture only becomes a control plane on this line, and the hook
        # under test branches on `config.UPDATE` as well as on the pin it re-reads. A test
        # that wanted the dev channel would write `[update]` into the file above and get it
        # here, rather than inheriting it from the developer's machine.
        config.use(self.root)

    def test_no_lock_does_nothing(self):
        """Opt-in: an unpinned control plane must behave exactly as before."""
        self._lock(None)
        self.assertIsNone(hooks._autosync_version_lock())
        self.assertEqual(self.installs, [])

    def test_matching_lock_does_nothing(self):
        from charter import __version__
        self._lock(__version__)
        self.assertIsNone(hooks._autosync_version_lock())
        self.assertEqual(self.installs, [])

    def test_drift_installs_the_locked_version(self):
        self._lock("9.9.9")
        msg = hooks._autosync_version_lock()
        self.assertEqual(self.installs, ["9.9.9"])
        self.assertIn("9.9.9", msg)

    def test_a_downgrade_is_reported_rather_than_installed(self):
        """The pin is committed data and this site is unattended, so the direction that
        can only remove guards is said, not done (#333 — full argument, and the attended
        path that still downgrades, in `test_a_version_lock_does_not_downgrade_unattended`)."""
        self._lock("0.0.1")
        msg = hooks._autosync_version_lock()
        self.assertEqual(self.installs, [])
        self.assertIn("0.0.1", msg)

    def test_the_message_says_the_running_process_is_still_the_old_build(self):
        """It cannot replace itself mid-call; without saying so, a user sees
        'installed' then `charter --version` reporting the old number."""
        self._lock("9.9.9")
        self.assertIn("next", hooks._autosync_version_lock().lower())

    def test_a_failed_install_warns_and_never_raises(self):
        """Being offline is not a reason to be unable to work."""
        commands.sync_to = lambda v: (False, "offline")
        self._lock("9.9.9")
        msg = hooks._autosync_version_lock()
        self.assertIn("failed", msg.lower())
        self.assertIn("charter version sync", msg)

    def test_a_broken_control_plane_never_raises(self):
        (self.root / "charter.toml").write_text("this is not toml {{{")
        self.assertIsNone(hooks._autosync_version_lock())


class SyncCommand(unittest.TestCase):
    def test_the_install_command_pins_exactly(self):
        cmd = commands._sync_cmd("1.2.3")
        self.assertIn("charter-cp==1.2.3", cmd)
        self.assertIn("--force", cmd)
        self.assertIn("--refresh", cmd)

    def test_it_never_offers_uv_tool_upgrade(self):
        """`uv tool upgrade` reports "Nothing to upgrade" for a git install and
        leaves the user pinned — the trap `make upgrade` exists to route around."""
        self.assertNotIn("upgrade", commands._sync_cmd("1.2.3"))


class CommitPushTakesGitsArgumentsNotACommandLine(unittest.TestCase):
    """`charter version bump --push` had never committed anything.

    It passed `["git", "add", rel]` to `commit_push`, whose `_git` helper supplies `git`
    itself — so the command run was `git git add charter.toml`. That fails; the staged-
    nothing check immediately below then took the "Nothing to save" branch and returned
    0; and the caller printed `✓ committed + pushed — teammates conform on their next
    session`. Seven other callers pass `["add", …]`, so the mistake was invisible by
    comparison and invisible at runtime.

    Asserted as a shape rather than fixed only at the one caller, because nothing between
    the wrong argv and the success message could tell the difference.
    """

    def test_a_leading_git_is_refused(self):
        with self.assertRaises(ValueError) as e:
            commands.commit_push(Path("/tmp"), ["git", "add", "charter.toml"], "m")
        self.assertIn("drop the leading 'git'", str(e.exception))

    def test_every_caller_in_the_tree_passes_bare_arguments(self):
        """The regression test for the fix itself: grep the callers, not the behaviour."""
        import re
        root = Path(__file__).resolve().parents[1] / "charter"
        bad = []
        for f in root.rglob("*.py"):
            for m in re.finditer(r"commit_push\(\s*[^,]+,\s*\[([^\]]*)\]", f.read_text()):
                if m.group(1).strip().startswith(('"git"', "'git'")):
                    bad.append(f"{f.name}: {m.group(0)[:60]}")
        self.assertEqual(bad, [], f"commit_push called with a command line, not arguments: {bad}")


class CommitPushRefusesToClaimSuccessItDidNotHave(PersonaIso):
    """`charter save` printed `✓ Committed : charter save: 0 file(s)` and exited 0 in a
    plane that is not a git repo — which `charter init` in a fresh directory produces, and
    which is exactly the README's 60-second path. Every git call here runs `check=False`
    (this is reached from hooks and background paths that must never break a turn), so the
    add failed silently, `diff --cached --quiet` returned 128 rather than 0 so the
    "Nothing to save" branch was skipped, the commit failed too, and `rev-parse` came back
    empty. The personas and memory charter had just told the user to commit had no
    history."""

    def test_a_plane_that_is_not_a_repo_is_refused(self):
        err = io.StringIO()
        with redirect_stderr(err):
            rc = commands.commit_push(self.tmp, ["add", "-A"], "m")
        self.assertEqual(rc, 1)
        self.assertIn("not a git repository", err.getvalue())

    def test_the_refusal_says_what_to_do(self):
        err = io.StringIO()
        with redirect_stderr(err):
            commands.commit_push(self.tmp, ["add", "-A"], "m")
        self.assertIn("git init", err.getvalue())

    def test_a_real_repo_with_nothing_staged_still_reports_nothing_to_save(self):
        import subprocess
        subprocess.run(["git", "init", "-q", str(self.tmp)], check=True, capture_output=True)
        err = io.StringIO()
        with redirect_stderr(err):
            rc = commands.commit_push(self.tmp, ["add", "-A"], "m")
        self.assertEqual(rc, 0)
        self.assertIn("Nothing to save", err.getvalue())


if __name__ == "__main__":
    unittest.main()
