"""A named ceiling that carries what to do about it.

`doctor` earned its deficit list by refusing to render a lower ceiling as health. The next
failure is subtler: a limit stated and left there reads as "nothing can be done", and the
operator stops looking. Where charter HAS an answer, the deficit carries it.

Where it has none, it says nothing rather than inventing one — an empty remedy is a claim
too, and a wrong workaround costs more than an honest gap.
"""

from __future__ import annotations

import unittest

from charter import doctor
from charter.harness import registry


class RemediesAreOffered(unittest.TestCase):
    def test_a_missing_status_bar_names_the_ambient_render(self):
        for name in ("opencode", "codex"):
            with self.subTest(harness=name):
                d = next(d for d in registry.get(name).deficits if d.key == "status-bar")
                self.assertIn("charter statusline --watch", d.remedy)

    def test_a_deficit_with_no_answer_offers_none(self):
        """opencode has no per-turn prompt hook and charter cannot conjure one — the
        nudges already ride tool output, which the `detail` explains. Inventing a remedy
        here would send someone off to configure something that does not exist."""
        d = next(d for d in registry.get("opencode").deficits if d.key == "prompt-hook")
        self.assertEqual(d.remedy, "")

    def test_every_remedy_is_a_command_the_operator_can_run(self):
        for h in registry.all():
            for d in h.deficits:
                if d.remedy:
                    with self.subTest(harness=h.name, key=d.key):
                        self.assertIn("charter ", d.remedy)


class DoctorShowsThem(unittest.TestCase):
    def test_the_row_carries_the_remedy_beside_the_ceiling(self):
        import os
        from unittest import mock
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode"}, clear=True):
            r = doctor.check_harness()
        self.assertIn("charter statusline --watch", r.detail)


if __name__ == "__main__":
    unittest.main()
