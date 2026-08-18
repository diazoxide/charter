"""A capture may fall one minor behind. Two is drift nobody notices.

`docs/assets/README.md` already draws the right line — captures "are generated from real
command output… none should ever be hand-edited — regenerate instead, so a screenshot cannot
quietly drift from what charter actually prints". Nothing enforced it, and `demo.svg` sat at
0.28-era output while charter shipped 0.39.0: the README showed a status line with columns
that no longer exist beside prose describing features it does not render.

**A check, not a release step.** Regenerating on every publish would put a terminal recorder
on the critical path, and a flaky capture would block a release — the same reasoning that
keeps `charter version sync` from running `claude plugin update` for you. So the suite
fails when a capture falls **more than one minor** behind, which leaves exactly one release
of slack: bump once and nothing happens, bump twice without regenerating and CI says so.

The stamp lives in `docs/assets/captured.json` rather than inside the SVGs. An SVG is
regenerated wholesale by `ansi2svg.py`, so a version comment inside it would be written by
the very step that is supposed to be recording it — and a stamp that updates itself proves
nothing.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

import charter

_REPO = Path(__file__).resolve().parent.parent
_STAMP = _REPO / "docs" / "assets" / "captured.json"

#: How many minors a capture may lag. One, so a release does not have to regenerate assets
#: to go out, but two consecutive ones cannot pass unnoticed.
MAX_MINOR_LAG = 1


def _minor(v: str) -> tuple[int, int]:
    parts = (v or "").split(".")
    return int(parts[0]), int(parts[1]) if len(parts) > 1 else 0


class TestTheStampIsUsable(unittest.TestCase):
    def test_it_exists(self):
        """Without it the check silently passes, which is the failure mode it exists to
        prevent."""
        self.assertTrue(_STAMP.is_file(), f"{_STAMP} is missing")

    def test_it_is_json_mapping_assets_to_versions(self):
        doc = json.loads(_STAMP.read_text())
        self.assertIsInstance(doc, dict)
        self.assertTrue(doc, "no assets stamped")

    def test_every_stamped_asset_actually_exists(self):
        doc = json.loads(_STAMP.read_text())
        for name in doc:
            self.assertTrue((_STAMP.parent / name).is_file(), f"{name} is stamped but absent")

    def test_every_capture_is_stamped(self):
        """A capture added without a stamp would be exempt from the check forever. The
        drawings (`model.svg`, `social-card.svg`) have no source to re-run and are
        deliberately not listed — see docs/assets/README.md."""
        doc = json.loads(_STAMP.read_text())
        captures = {"demo.svg", "personas.svg", "statusline.svg"}
        self.assertEqual(captures - set(doc), set(),
                         "a capture exists with no recorded version")


class TestTheCapturesAreNotStale(unittest.TestCase):
    def test_no_capture_lags_by_more_than_one_minor(self):
        now = _minor(charter.__version__)
        stale = []
        for name, at in json.loads(_STAMP.read_text()).items():
            was = _minor(at)
            lag = (now[0] - was[0]) * 1000 + (now[1] - was[1])
            if lag > MAX_MINOR_LAG:
                stale.append(f"{name} captured at {at}, charter is {charter.__version__}")
        self.assertEqual(stale, [], "regenerate: see docs/assets/README.md, then update "
                                    "docs/assets/captured.json — " + "; ".join(stale))


if __name__ == "__main__":
    unittest.main()
