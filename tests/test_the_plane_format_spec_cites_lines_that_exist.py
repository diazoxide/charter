"""Every `module.py:line` in `docs/plane-format.md` points at a line that is really there.

The plane-format spec is the contract the Rust rebuild reads and writes this plane by
(ADR 0025, spec decision 13), and it earns that by citing the code for every claim: some
1,600 citations of the form `charter/workspace.py:1418`. A line number is the one kind of
claim that rots on its own — nobody has to touch the document for it to go wrong, only to
insert a line above the one it points at.

**What this test is, honestly.** It is a pointer check, not a truth check. It cannot tell
whether `charter/config.py:592` still holds the temp-name rule the spec says it does; what
it can tell is that the file exists, that the line exists, and that it is not blank — the
three ways a citation rots when code moves under it. A blank line is included because it is
the signature of a small upward drift: the citation slid off the end of a function into the
gap before the next one, which is exactly what happened to five citations while this
document was being assembled.

The spec is frozen against a commit (`50d31dc`), and the Python charter is frozen with it
(spec decision 16), so this test going red means one of two things: somebody moved code the
spec cites, or somebody edited the spec's numbers by hand. Either way the document and the
code have parted company, and the fix is to re-derive the numbers, never to delete the test.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SPEC = REPO / "docs" / "plane-format.md"

#: `charter/frame/state.py:2084`, with or without the surrounding backticks.
CITATION = re.compile(r"(charter/[\w/]+\.py):(\d+)")


class ThePlaneFormatSpecCitesLinesThatExist(unittest.TestCase):
    def setUp(self) -> None:
        self.citations = [
            (n, m.group(1), int(m.group(2)))
            for n, line in enumerate(SPEC.read_text().splitlines(), 1)
            for m in CITATION.finditer(line)
        ]

    def test_the_spec_cites_the_code_at_all(self) -> None:
        """A spec that stopped citing code would pass every check below vacuously."""
        self.assertGreater(len(self.citations), 1000, "the spec has lost its citations")

    def test_every_citation_points_at_a_line_that_exists_and_is_not_blank(self) -> None:
        source: dict[str, list[str]] = {}
        rotten: list[str] = []
        for doc_line, module, line_no in self.citations:
            path = REPO / module
            if not path.is_file():
                rotten.append(f"plane-format.md:{doc_line} cites {module}, which is gone")
                continue
            lines = source.setdefault(module, path.read_text().splitlines())
            where = f"plane-format.md:{doc_line} cites {module}:{line_no}"
            if not 1 <= line_no <= len(lines):
                rotten.append(f"{where}, but that file ends at line {len(lines)}")
            elif not lines[line_no - 1].strip():
                rotten.append(f"{where}, which is blank — the code moved under it")
        self.assertEqual([], rotten, "\n".join(rotten))


if __name__ == "__main__":
    unittest.main()
