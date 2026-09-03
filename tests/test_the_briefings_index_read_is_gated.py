"""`MEMORY.md` is read into the briefing, and it was the one plane file nothing gated.

Found by the agent closing #349 and handed over, because the read lives in `hooks.py`.

#336 gated every read of plane data behind `contain.file_refusal`, and #349 gated the
writes. `memstore.files()` implements the read gate — and excludes `MEMORY.md` **by
name**, deliberately, because the index is not a memory. `hooks._index_titles` then opened
that exact filename with nothing in front of it:

    return [ln for ln in idx_path.read_text().splitlines() if ln.startswith("- [")]
    except OSError:

So the store's single most predictable filename was the one path neither gate covered, on
a hook that runs at every session start. A committed symlink there redirects the read into
a file the `pretooluse-read` vault guard exists to keep out of a system prompt, and a FIFO
does not raise `OSError` at all — it **blocks**, which costs the session its briefing and
the turn with it.

**What a refused index does to the briefing, since that is the part with a choice in it.**
Not silence. The memory *count* comes from `memstore.files()`, which is gated separately
and still answers, so a persona with twelve memories and a refused index would otherwise
read as a persona with twelve memories and nothing worth showing — the same shape as one
whose index is simply empty. The refusal is rendered in place of the titles instead, in
the store's own vocabulary, so the reader learns there is a defect in a committed file
rather than inferring there is nothing to see. `charter recall` is unaffected either way:
it reads the memory files, not the index.
"""

from __future__ import annotations

import os
import unittest

from unittest import mock

from charter import config, contain, hooks, persona
from tests._isolation import PersonaIso, PlaneIso


class TheIndexIsPlaneDataToo(PlaneIso):
    def setUp(self) -> None:
        super().setUp()
        self.make_persona("helper")
        persona.remember("helper", "a fact about the release guard")
        persona.remember("helper", "a second fact, distinctly worded")
        self.idx = persona.index_of(persona.memory_dir("helper"))
        self.assertTrue(self.idx.exists(), "precondition: no index was scaffolded")

    def _digest(self) -> str:
        return hooks._memory_digest("helper")

    # -- precondition -------------------------------------------------------- #

    def test_an_ordinary_index_is_still_read(self):
        """PRECONDITION for every refusal below: if the titles stop arriving, "no titles"
        stops meaning "refused"."""
        titles = hooks._index_titles(self.idx)
        self.assertEqual(len(titles), 2)
        self.assertIn("release guard", self._digest())

    # -- the gate ------------------------------------------------------------ #

    def test_an_index_symlinked_out_of_the_plane_is_refused(self):
        outside = config.STATE_DIR / "vaults" / "devops.json"
        outside.parent.mkdir(parents=True, exist_ok=True)
        outside.write_text('- [TOKEN-CANARY](x.md)\n{"token": "s3cr3t"}\n')
        self.idx.unlink()
        self.idx.symlink_to(outside)
        self.assertNotIn("TOKEN-CANARY", "".join(hooks._index_titles(self.idx)))
        self.assertNotIn("TOKEN-CANARY", self._digest())

    def test_an_index_symlinked_inside_the_plane_is_still_followed(self):
        """The legitimate case #348 kept working: a plane may link its data about."""
        elsewhere = persona.memory_dir("helper") / "real-index.md"
        elsewhere.write_text("- [LINKED-TITLE](note.md)\n")
        self.idx.unlink()
        self.idx.symlink_to(elsewhere)
        self.assertIn("LINKED-TITLE", "".join(hooks._index_titles(self.idx)))

    def test_a_fifo_index_does_not_block_the_briefing(self):
        """A FIFO raises nothing — it waits. The gate answers from `lstat`, so there is
        nothing to time out because nothing is opened."""
        self.idx.unlink()
        os.mkfifo(self.idx)
        self.addCleanup(self.idx.unlink)
        self.assertEqual(hooks._index_titles(self.idx), [])
        self._digest()          # must return, not hang

    def test_an_oversized_index_is_refused(self):
        self.idx.write_text("- [BIG](x.md)\n" + "x" * (contain.MAX_BYTES + 1))
        self.assertNotIn("BIG", "".join(hooks._index_titles(self.idx)))

    # -- what the reader is told -------------------------------------------- #

    def test_a_refused_index_is_named_rather_than_read_as_empty(self):
        self.idx.unlink()
        os.mkfifo(self.idx)
        self.addCleanup(self.idx.unlink)
        digest = self._digest()
        self.assertIn("own (2)", digest, "the memory count must still be honest")
        self.assertRegex(digest, r"(?i)could not be read|not a regular file|refus")

    def test_an_unreadable_index_never_raises(self):
        self.idx.unlink()
        self.assertEqual(hooks._index_titles(self.idx), [])
        self._digest()

    def test_the_session_hook_still_emits_a_briefing(self):
        """A hook may cost a session its titles and never its turn."""
        from tests._isolation import run_hook
        self.idx.unlink()
        os.mkfifo(self.idx)
        self.addCleanup(self.idx.unlink)
        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "helper",
                                                   "CHARTER_WORKSPACE": "default"}):
            out = run_hook(hooks.sessionstart, {"session_id": "t"})
        self.assertIsNotNone(out, "the briefing was lost to a refused index")


if __name__ == "__main__":

    unittest.main()
