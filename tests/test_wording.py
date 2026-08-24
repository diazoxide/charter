"""Words charter is not allowed to use about what it does.

A security claim that is false in a reachable configuration is worse than no claim, and
the cheapest way to make one is a verb. `_safe_unlink` is `os.unlink` — no overwrite
pass, and it should not grow one: rewriting a block on APFS, on ext4 with a journal, or
on any SSD doing wear levelling writes a NEW block and leaves the old contents wherever
the drive left them. So the code is right and the word was wrong, in six places across
`charter/` and `skills/` (`cli.py`, `commands_secrets.py` ×3, `persona.py`, the browser
skill ×2), each of which told a reader that a temp credential had been destroyed rather
than unlinked.

Kept as a grep rather than a comment because the word is *attractive*: it is what shell
scripts do (`shred(1)`), it is what the operation feels like, and the next person to
describe this cleanup will reach for it again.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

#: Where charter speaks to a user or to the next maintainer.
SCANNED = ("charter", "skills")

#: "shred", "shreds", "shredded", "shredding" — as a word, so `shredder.py` or a URL
#: containing the letters is not a hit.
_SHRED = re.compile(r"\bshred(s|ded|ding)?\b", re.IGNORECASE)


def _sources():
    for top in SCANNED:
        for path in sorted((ROOT / top).rglob("*")):
            if path.is_file() and path.suffix in (".py", ".md", ".toml", ".json"):
                yield path


class TestNothingClaimsToShred(unittest.TestCase):
    def test_no_doc_or_docstring_claims_to_shred(self):
        hits = []
        for path in _sources():
            try:
                text = path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):       # pragma: no cover - binaries
                continue
            for n, line in enumerate(text.splitlines(), 1):
                if _SHRED.search(line):
                    hits.append(f"{path.relative_to(ROOT)}:{n}: {line.strip()}")
        self.assertEqual(hits, [], "\n".join(
            ["charter deletes these files, it does not shred them — an overwrite pass "
             "is meaningless on a copy-on-write filesystem, so fix the word, not the "
             "code:"] + hits))

    def test_the_scan_actually_reads_files(self):
        """A rglob that matched nothing would make the assertion above vacuous — the
        failure mode this whole file exists to catch, applied to itself."""
        found = list(_sources())
        self.assertGreater(len(found), 20)
        self.assertIn(ROOT / "charter" / "commands_secrets.py", found)

    def test_the_pattern_would_catch_the_wording_it_replaced(self):
        original = ("Use --stream instead — it forks, inherits stdio, and shreds the "
                    "file when the child exits.")
        self.assertTrue(_SHRED.search(original))
        self.assertFalse(_SHRED.search(original.replace("shreds", "deletes")))


if __name__ == "__main__":
    unittest.main()
