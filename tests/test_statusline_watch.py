"""`charter statusline --watch` — an ambient plane state on a harness that has no bar.

Naming a ceiling is honest; leaving it there is half a job. Claude Code renders the status
line every turn because it has a socket for one. opencode has none, and Codex's
`tui.status_line` takes a list of built-in segments rather than a command — so on both, the
plane state has nowhere ambient to live. This puts it in any spare terminal, which needs no
socket and no multiplexer, and works the same on every harness including the one that does
have a bar.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stdout
from unittest import mock

from charter import statusline
from tests import _envguard


class Watch(unittest.TestCase):
    def _run(self, frames: int = 3, **kw) -> str:
        """Drive the loop for *frames* repaints, then interrupt it the way Ctrl-C does."""
        calls = {"n": 0}

        def fake_sleep(_seconds):
            calls["n"] += 1
            if calls["n"] >= frames:
                raise KeyboardInterrupt

        buf = io.StringIO()
        with mock.patch.object(statusline.time, "sleep", fake_sleep), \
                mock.patch.object(statusline, "render", return_value="PLANE"), \
                redirect_stdout(buf):
            self.rc = statusline.watch(**kw)
        return buf.getvalue()

    def test_it_repaints_until_interrupted_and_exits_cleanly(self):
        out = self._run(frames=3)
        self.assertEqual(self.rc, 0)
        self.assertEqual(out.count("PLANE"), 3)

    def test_it_repaints_in_place_rather_than_scrolling(self):
        """A status line that scrolls is a log. The point of an ambient render is that it
        occupies the same rows every time, so a glance costs nothing."""
        out = self._run(frames=2)
        self.assertIn("\033[H", out)

    def test_it_leaves_the_cursor_visible_after_ctrl_c(self):
        """Hiding the cursor and dying without restoring it leaves the operator's terminal
        broken, which is a worse failure than the one this feature fixes."""
        out = self._run(frames=1)
        self.assertIn("\033[?25l", out)
        self.assertTrue(out.rstrip().endswith("\033[?25h")
                        or "\033[?25h" in out.split("PLANE")[-1])

    def test_it_says_what_it_cannot_show(self):
        """There is no session payload here, so the token and context columns are blank.
        A render that looks like the real thing while silently omitting a column teaches
        the reader to trust a number that is not there."""
        out = self._run(frames=1)
        self.assertIn("no session", out.lower())

    def test_the_render_is_never_allowed_to_kill_the_loop(self):
        buf = io.StringIO()
        with mock.patch.object(statusline.time, "sleep", side_effect=KeyboardInterrupt), \
                mock.patch.object(statusline, "render", side_effect=RuntimeError("boom")), \
                redirect_stdout(buf):
            self.assertEqual(statusline.watch(), 0)


class WatchIsReachableFromTheCli(unittest.TestCase):
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

    def test_the_flag_routes_to_the_loop(self):
        with mock.patch.object(statusline, "watch", return_value=0) as w:
            self.assertEqual(statusline.main(["--watch"]), 0)
        w.assert_called_once()

    def test_the_interval_is_the_operators_to_choose(self):
        with mock.patch.object(statusline, "watch", return_value=0) as w:
            statusline.main(["--watch", "--interval", "5"])
        self.assertEqual(w.call_args.kwargs.get("interval"), 5.0)

    def test_without_the_flag_it_still_reads_a_payload_and_prints_once(self):
        with mock.patch.object(statusline, "render", return_value="ONCE"):
            buf = io.StringIO()
            with mock.patch("sys.stdin", io.StringIO("{}")), redirect_stdout(buf):
                statusline.main([])
        self.assertEqual(buf.getvalue().strip(), "ONCE")


if __name__ == "__main__":
    unittest.main()
