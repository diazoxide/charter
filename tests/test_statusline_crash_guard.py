"""FINDING M9 — the module docstring says the status line "must never crash", and
`render()`'s outer `try/except` was supposed to guarantee that. But `_repo_rows` and
`_persona_chips` were called AFTER that `try/except` had already closed (between it and
a SECOND, narrower `try/except` that only wraps the final layout step) — so a bad repo
row (or a broken persona) crashed `render()` itself, propagating straight out. `main()`
had no guard at all around its call to `render()` either, so the same crash would take
down the whole status-line subprocess. This is the exact failure mode the docstring
promises can never happen."""
from __future__ import annotations

import io
import json
import shutil
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

from charter import statusline


class RenderNeverCrashesCase(unittest.TestCase):
    def setUp(self):
        from charter import config
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-renderguard-"))
        self._orig = config.SESSIONS_DIR
        config.SESSIONS_DIR = self.tmp / "sessions"
        # Also redirect STATE_DIR: these render against the REAL plane on purpose,
        # and the render path now writes a vault-health cache under it. A test that
        # writes into the developer's own `.charter/` is the bug this suite fixed
        # once already (see `EveryRootDerivedPathIsIsolated`).
        _state = config.STATE_DIR
        config.STATE_DIR = self.tmp / ".charter"
        self.addCleanup(lambda: setattr(config, "STATE_DIR", _state))
        self.addCleanup(lambda: (setattr(config, "SESSIONS_DIR", self._orig),
                                 shutil.rmtree(self.tmp, ignore_errors=True)))

    def test_a_broken_repo_row_does_not_crash_render(self):
        """`_repo_rows` sits between the two try/except blocks — mock it to raise and
        prove `render()` still returns the minimal fallback instead of propagating."""
        with mock.patch.object(statusline, "_repo_rows", side_effect=RuntimeError("boom")):
            out = statusline.render({"session_id": "t"})
        self.assertIn("charter", out)

    def test_a_broken_persona_chips_call_does_not_crash_render(self):
        with mock.patch.object(statusline, "_persona_chips", side_effect=RuntimeError("boom")):
            out = statusline.render({"session_id": "t"})
        self.assertIn("charter", out)

    def test_main_never_crashes_even_if_render_itself_raises(self):
        """`main()` had NO guard at all around `render(payload)` — a defense-in-depth
        gap even after `render()` is fixed, since a future bug inside `render()` (or a
        monkeypatch, or an import-time surprise) would otherwise take the whole
        status-line subprocess down with it."""
        buf = io.StringIO()
        with mock.patch.object(statusline, "render", side_effect=RuntimeError("boom")), \
             mock.patch("sys.stdin", io.StringIO(json.dumps({"session_id": "t"}))), \
             redirect_stdout(buf):
            rc = statusline.main()
        self.assertEqual(rc, 0)
        self.assertIn("charter", buf.getvalue())


if __name__ == "__main__":
    unittest.main()
