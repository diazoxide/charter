"""What travels upstream in a **bug** report, and what must never.

The payload is a **closed allowlist** — nothing reaches a public issue unless it was
explicitly decided to be safe. A blocklist leaks eventually; an allowlist fails closed.
The Reporter's session holds absolute paths carrying their username, private repo and org
names, workspace and persona names; a public tracker is the wrong place to discover any of
them (see docs/adr/0001, and `workspaces/user-reporting/workspace.md` for the glossary).
"""
from __future__ import annotations

import traceback
import unittest
from pathlib import Path

import charter
from charter import report


def _caught(exc: Exception) -> Exception:
    """An exception with a real ``__traceback__``, the way `cli.main` would see it."""
    try:
        raise exc
    except Exception as e:  # noqa: BLE001 - the point is to capture whatever was raised
        return e


class TestBugPayloadIsAClosedAllowlist(unittest.TestCase):
    def test_payload_carries_exactly_the_allowlisted_fields(self):
        """Six mechanical fields plus `message`, the one free-text field the Reporter
        must read. Asserted as an exact set, not a subset: a field added without a
        decision should fail here rather than surface on a public issue."""
        p = report.bug_payload(_caught(ValueError("boom")), subcommand="clone")
        self.assertEqual(
            set(p),
            {"charter_version", "python_version", "os", "subcommand",
             "exception_type", "frames", "message"},
        )

    def test_argv_never_appears_in_a_payload(self):
        """The subcommand name is safe; its ARGUMENTS are not — they carry workspace,
        repo and org names. Excluded deliberately, so this is a named guard rather than
        a consequence of the set above."""
        p = report.bug_payload(_caught(ValueError("boom")), subcommand="clone")
        self.assertNotIn("argv", p)
        self.assertEqual(p["subcommand"], "clone")

    def test_exception_type_is_recorded_by_name(self):
        p = report.bug_payload(_caught(KeyError("k")), subcommand="doctor")
        self.assertEqual(p["exception_type"], "KeyError")


class TestFramesAreCharterOnlyAndRelativized(unittest.TestCase):
    """A traceback is the leakiest thing charter captures. Absolute paths carry the
    Reporter's username; frames from *their* code carry their filenames and function
    names. Only charter's own frames survive, and only as package-relative paths."""

    def setUp(self) -> None:
        self.pkg = Path(charter.__file__).parent

    def test_a_charter_frame_survives_as_a_package_relative_path(self):
        f = traceback.FrameSummary(str(self.pkg / "cli.py"), 713, "main")
        self.assertEqual(report.safe_frames([f]), ["charter/cli.py:713 in main"])

    def test_the_reporters_own_frames_are_dropped_entirely(self):
        """Not relativized — dropped. Their filenames and function names are theirs."""
        mine = traceback.FrameSummary(str(Path.home() / "code/billing/app.py"), 3, "charge")
        ours = traceback.FrameSummary(str(self.pkg / "workspace.py"), 42, "ensure")
        self.assertEqual(report.safe_frames([mine, ours]), ["charter/workspace.py:42 in ensure"])

    def test_stdlib_frames_are_dropped(self):
        std = traceback.FrameSummary("/usr/lib/python3.14/subprocess.py", 1, "run")
        self.assertEqual(report.safe_frames([std]), [])

    def test_a_nested_charter_module_keeps_its_subpath(self):
        """`charter/forge/github.py`, not `github.py` — the subpackage is the useful part."""
        f = traceback.FrameSummary(str(self.pkg / "forge" / "github.py"), 66, "_api")
        self.assertEqual(report.safe_frames([f]), ["charter/forge/github.py:66 in _api"])

    def test_no_absolute_path_survives(self):
        """The guard that matters: whatever the rule, nothing leaves with a leading `/`."""
        frames = [
            traceback.FrameSummary(str(Path.home() / "secret/thing.py"), 1, "f"),
            traceback.FrameSummary(str(self.pkg / "cli.py"), 713, "main"),
        ]
        for rendered in report.safe_frames(frames):
            self.assertFalse(rendered.startswith("/"), rendered)
            self.assertNotIn(str(Path.home()), rendered)


class TestPayloadUsesSafeFrames(unittest.TestCase):
    def test_frames_in_a_payload_are_rendered_strings(self):
        """The payload is what gets published, so its frames must already be safe —
        not FrameSummary objects a later caller has to remember to sanitize."""
        p = report.bug_payload(_caught(ValueError("boom")), subcommand="clone")
        for f in p["frames"]:
            self.assertIsInstance(f, str)


class TestFingerprintIdentifiesTheSameBug(unittest.TestCase):
    """The fingerprint is what collapses a crash loop into one report with a counter, and
    what lets an already-filed bug point at its own upstream issue without an API call."""

    def setUp(self) -> None:
        self.pkg = Path(charter.__file__).parent

    def _payload(self, exc_type="ValueError", message="boom", line=713):
        return {
            "charter_version": "0.19.0", "python_version": "3.11.0", "os": "macOS",
            "subcommand": "clone", "exception_type": exc_type, "message": message,
            "frames": report.safe_frames(
                [traceback.FrameSummary(str(self.pkg / "cli.py"), line, "main")]),
        }

    def test_the_same_failure_fingerprints_the_same(self):
        self.assertEqual(report.fingerprint(self._payload()),
                         report.fingerprint(self._payload()))

    def test_a_different_exception_type_is_a_different_bug(self):
        self.assertNotEqual(report.fingerprint(self._payload(exc_type="ValueError")),
                            report.fingerprint(self._payload(exc_type="KeyError")))

    def test_a_different_frame_is_a_different_bug(self):
        self.assertNotEqual(report.fingerprint(self._payload(line=713)),
                            report.fingerprint(self._payload(line=42)))

    def test_the_message_does_not_change_the_fingerprint(self):
        """The load-bearing one. Messages carry the variable part — `no workspace 'a'`
        versus `no workspace 'b'` is ONE bug. Folding the message in would defeat
        collapse entirely, and would put Reporter-specific names into a stored key."""
        self.assertEqual(report.fingerprint(self._payload(message="no workspace 'alpha'")),
                         report.fingerprint(self._payload(message="no workspace 'beta'")))

    def test_a_crash_with_no_charter_frames_still_fingerprints(self):
        """Every frame can be filtered out; that must not raise. It is still a real bug
        and still needs to collapse against its own repeats."""
        p = self._payload()
        p["frames"] = []
        self.assertTrue(report.fingerprint(p))
        self.assertEqual(report.fingerprint(p), report.fingerprint(p))

    def test_a_fingerprint_leaks_nothing_readable(self):
        """It is stored and compared, so it must not become a side channel for the
        message it was deliberately built without."""
        fp = report.fingerprint(self._payload(message="/Users/someone/private-repo"))
        self.assertNotIn("someone", fp)
        self.assertNotIn("private-repo", fp)


if __name__ == "__main__":
    unittest.main()
