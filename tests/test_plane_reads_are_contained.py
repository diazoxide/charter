"""A committed symlink must not redirect a read of plane data out of the plane's data.

#336(a): charter reads plane data by listing a directory and opening what it finds, and
**nothing called `is_symlink`, `lstat` or `realpath`** on the way — in `persona.py`,
`memstore.py`, `recall.py`, and therefore in `curate.py` and `todos.py`, which read
through the store. Git commits symlinks, so the target of every one of those reads is
attacker-controlled from a commit.

The demonstration that matters is the one that walks around a guard charter already
ships: `personas/reader/persona.md` → `../../.charter/vaults/devops.json` makes charter
read a vault file and `sync-agents` write its contents into a sub-agent's system prompt,
while `pretooluse-read` (`hooks._VAULT_PATH_RE`) denies the agent reading that same file
directly. The path is **plane-relative** and needs no knowledge of the victim's home.

**The boundary is the plane's DATA roots, not the plane root.** `.charter/` sits *under*
`ROOT` (`config._migrate_state_dir` → `root / ".charter"`), so the obvious rule — "refuse
a path whose realpath leaves the plane" — admits the flagship demonstration unchanged:
`../../.charter/vaults/devops.json` never leaves the plane. What charter may read is
`personas/`, `workspaces/` and `PERSONA_STATE_DIR` (ephemeral memory, which lives *inside*
`.charter/` and must keep working). `test_a_refs_directory_symlinked_into_the_state_dir…`
and `test_ephemeral_memory_under_the_state_dir…` are the two halves of that correction,
and they fail in opposite directions if the boundary is drawn at ROOT or at `.charter`.

**Escaping links are refused; links are not.** `contain`'s lexical containment was chosen
in #342 partly so it would not refuse a plane that legitimately symlinks a persona
directory. Resolving preserves that case — a link landing inside the data roots is
followed, and the benign half of this file is what catches a "fix" that contains reads by
refusing all of them.

**Preconditions are asserted, not assumed.** Every case plants a real canary behind a real
symlink and asserts the OS follows it *before* asking charter to refuse — a refusal
because the fixture was a copy, or the target was missing, would prove nothing. This audit
has produced six vacuous passes already.
"""

from __future__ import annotations

import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from charter import commands_persona, config, memstore, persona, recall
from tests._isolation import PersonaIso

#: The issue's own canary, kept verbatim so a grep from the report lands here.
CANARY = "CANARY-LOCAL-VAULT-SECRET-4d8e"


class PlaneReadsAreContained(PersonaIso):

    def setUp(self) -> None:
        super().setUp()
        # A plain-file vault, at the fixed place relative to the plane that makes the
        # attack portable. `.charter/vaults/` is exactly what `_VAULT_PATH_RE` denies the
        # agent, and exactly what charter itself walked into.
        self.vault = config.VAULTS_DIR / "devops.json"
        self.vault.parent.mkdir(parents=True, exist_ok=True)
        self.vault.write_text(json.dumps({"token": CANARY}))
        self.outside = Path(tempfile.mkdtemp(prefix="edm-test-outside-"))
        self.addCleanup(shutil.rmtree, self.outside, ignore_errors=True)

    # ------------------------------------------------------------------ helpers
    def link(self, link: Path, target: Path) -> str:
        """Plant *link* → *target* as a **relative** symlink and prove it is one.

        Relative because that is what makes a committed link portable to a machine whose
        layout the author does not know — and asserting `is_symlink` is what stops this
        file passing on a fixture that is quietly a copy.
        """
        link.parent.mkdir(parents=True, exist_ok=True)
        rel = os.path.relpath(target, link.parent)
        os.symlink(rel, link)
        self.assertTrue(link.is_symlink(), f"fixture must be a symlink, not a copy: {link}")
        return rel

    def escaping_link(self, link: Path, target: Path) -> str:
        rel = self.link(link, target)
        self.assertTrue(rel.startswith(".."),
                        f"fixture must escape its own directory, got {rel!r}")
        return rel

    # ------------------------------------------------------- (a) the redirection
    def test_a_persona_charter_symlinked_to_a_vault_is_not_read_into_an_agent(self):
        d = config.PERSONAS_DIR / "reader"
        d.mkdir(parents=True)
        rel = self.escaping_link(d / "persona.md", self.vault)

        # Preconditions: the link is live, plane-relative, followed by the OS, and charter
        # still SEES the persona — so a refusal below is charter's, not the filesystem's.
        self.assertIn(CANARY, (d / "persona.md").read_text(),
                      f"precondition: the OS must follow {rel!r} to the vault")
        self.assertIn("reader", persona.list_personas())

        self.assertIsNone(persona.load("reader"),
                          "a definition resolving outside the plane's data must not load")

        commands_persona.cmd_persona_sync_agents(
            SimpleNamespace(persona="reader", approve_mcp=False))
        generated = config.ROOT / ".claude" / "agents" / "reader.md"
        body = generated.read_text() if generated.exists() else ""
        self.assertNotIn(CANARY, body,
                         "sync-agents wrote a vault into a sub-agent's system prompt")

    def test_a_memory_symlinked_to_a_vault_is_not_recalled(self):
        self.make_persona("reader")
        mem = persona.memory_dir("reader")
        (mem / "kept.md").write_text("# a real memory\n\nbadger badger\n")
        self.escaping_link(mem / "leak.md", self.vault)
        self.assertIn(CANARY, (mem / "leak.md").read_text(), "precondition: link is live")

        listed = [p.name for p in memstore.files(mem)]
        self.assertNotIn("leak.md", listed)
        self.assertIn("kept.md", listed, "the store must still list its real memories")

        hits = recall.recall("canary", persona_name="reader", scopes=("persona",)).hits
        self.assertEqual([], [h.path.name for h in hits])

    def test_a_ref_directory_symlinked_out_of_the_plane_is_not_a_source(self):
        self.make_persona("reader")
        elsewhere = self.outside / "notes"
        elsewhere.mkdir(parents=True)
        (elsewhere / "elsewhere.md").write_text(f"# elsewhere\n\n{CANARY}\n")
        refs = persona.refs_dir("reader")
        refs.mkdir(parents=True, exist_ok=True)
        self.link(refs / "linked", elsewhere)
        self.assertIn(CANARY, (refs / "linked" / "elsewhere.md").read_text(),
                      "precondition: rglob's is_dir() follows this link")

        self.assertNotIn(refs / "linked", recall._ref_dirs(refs))
        hits = recall.recall("elsewhere", persona_name="reader", scopes=("refs",)).hits
        self.assertEqual([], [h.path.name for h in hits])

    def test_a_refs_directory_symlinked_into_the_state_dir_is_refused(self):
        """The case a "must not leave the plane" rule would wave through.

        `.charter/reports/` holds drafted upstream reports — "a draft may quote an
        exception message that has not been redacted yet" (`config.derive`) — and it sits
        *under* ROOT. Containment drawn at the plane root admits this whole directory.
        """
        reports = config.STATE_DIR / "reports"
        reports.mkdir(parents=True, exist_ok=True)
        (reports / "draft.md").write_text(f"# draft\n\n{CANARY}\n")
        self.assertTrue(str(reports).startswith(str(config.ROOT)),
                        "precondition: the state dir is INSIDE the plane root")

        self.make_persona("reader")
        refs = persona.refs_dir("reader")
        shutil.rmtree(refs, ignore_errors=True)
        self.escaping_link(refs, reports)
        self.assertIn(CANARY, (refs / "draft.md").read_text(), "precondition: link is live")

        hits = recall.recall("draft", persona_name="reader", scopes=("refs",)).hits
        self.assertEqual([], [h.path.name for h in hits])

    def test_lint_says_why_a_refused_definition_did_not_load(self):
        """A refusal nobody can see is how #337 happened: `lint` called a grant dangling
        while the resolver honoured it. A definition charter declines to read has to say
        so in the signal an operator actually checks, not silently become an empty
        persona."""
        d = config.PERSONAS_DIR / "reader"
        d.mkdir(parents=True)
        self.escaping_link(d / "persona.md", self.vault)
        self.assertIn(CANARY, (d / "persona.md").read_text(), "precondition: link is live")

        issues = persona.structural_errors("reader")
        self.assertTrue(
            any(level == "error" and "persona.md" in msg for level, msg in issues),
            f"lint must report the refused definition, got {issues}")

        # And through `lint`, which is what `charter persona lint` actually prints. Its
        # early return said only "persona 'reader' does not load" — true, and it sends the
        # reader to look for a file that is right there.
        printed = persona.lint("reader", deep=False)
        self.assertTrue(
            any("outside the directories" in msg for _level, msg in printed),
            f"lint must say WHY it did not load, got {printed}")

    # ------------------------------------------------------------ (a) the benign half
    def test_a_persona_directory_symlinked_inside_the_plane_still_loads(self):
        """#342 kept containment lexical partly to avoid breaking this plane. A resolving
        check keeps it working: the link lands inside `personas/`."""
        self.make_persona("real")
        self.link(config.PERSONAS_DIR / "alias", config.PERSONAS_DIR / "real")
        d = persona.load("alias")
        self.assertIsNotNone(d, "a link landing inside the plane's data must be followed")
        self.assertIn("charter body", d["charter"])

    def test_a_persona_directory_symlinked_out_of_the_plane_is_refused(self):
        """The other side of the fast path in `contain.within_data`: a directory whose
        parent IS a data root skips the resolve **only** when it is not a link. This one
        is, so it must fall through and be refused — every file inside it is an ordinary
        regular file that the per-file check has nothing to object to."""
        elsewhere = self.outside / "borrowed"
        elsewhere.mkdir(parents=True)
        (elsewhere / "persona.md").write_text(f"---\nname: borrowed\n---\n\n{CANARY}\n")
        self.link(config.PERSONAS_DIR / "borrowed", elsewhere)
        self.assertIn(CANARY, (config.PERSONAS_DIR / "borrowed" / "persona.md").read_text(),
                      "precondition: the link is live and the file inside it is ordinary")

        self.assertIsNone(persona.load("borrowed"))

    def test_a_memory_symlinked_to_another_in_plane_memory_is_still_recalled(self):
        self.make_persona("a")
        self.make_persona("b")
        (persona.memory_dir("b") / "fact.md").write_text("# shared fact\n\nbadger\n")
        self.link(persona.memory_dir("a") / "fact.md", persona.memory_dir("b") / "fact.md")
        hits = recall.recall("badger", persona_name="a", scopes=("persona",)).hits
        self.assertEqual(["fact.md"], [h.path.name for h in hits])

    def test_ephemeral_memory_under_the_state_dir_is_still_read(self):
        """Ephemeral memory lives in `PERSONA_STATE_DIR`, i.e. *inside* `.charter/`. A
        boundary drawn by banning the state directory silently empties this scope."""
        self.make_persona("a")
        eph = persona.ephemeral_dir("a", session="s1")
        eph.mkdir(parents=True, exist_ok=True)
        (eph / "scratch.md").write_text("# scratch\n\nbadger\n")
        self.assertTrue(str(eph).startswith(str(config.STATE_DIR)),
                        "precondition: ephemeral memory is inside the state dir")

        hits = recall.recall("badger", persona_name="a", session_id="s1",
                             scopes=("ephemeral",)).hits
        self.assertEqual(["scratch.md"], [h.path.name for h in hits])


if __name__ == "__main__":
    unittest.main()
