"""The loose-directory note reaches `charter doctor`, not only `charter vault list` (#471).

`PlainFileProvider.health()` returns ``(healthy, detail)`` and put the note in *detail*::

    1 secret(s), listed by other accounts: .charter 755 (want 700 — chmod 700)

`charter vault list` prints that detail as its STATUS column. `check_vaults` did
``healthy, _ = prov.health()`` and dropped it — and `_loose_dir_note` deliberately never
sets ``healthy`` False, because a directory another account can list is not an unreachable
vault and this check runs from the SessionStart hook, where it must not hold up a session.
So there was no path at all by which the note reached the command whose entire job is
surfacing exactly this class of thing.

**The fix is a structured question, not a substring.** `doctor` asks
`VaultProvider.loose_dirs()` for ``(path, mode)`` pairs and renders them through
`base.loose_dir_note`, which is also what `health()` renders. Scraping `health()`'s
sentence from `doctor` would have been the bypass this codebase keeps paying for: the
report would go silent the day somebody rewords the sentence, and nothing would fail. The
cases below assert the two surfaces carry **the same rendering of the same list**, rather
than asserting the prose twice.

**It stays a green line.** Charter will not chmod a directory it did not create — a
vault's ``file`` may name any path on this machine, and ``$CHARTER_HOME`` may move the
state directory anywhere — so reporting *is* the remedy, and a WARN at every session start
about a directory the operator has decided to leave alone is a check crying wolf. The note
is on the ✓ line, in the same posture as the "points outside the plane" note beside it.

The next spelling: a provider whose data lives in a directory charter creates and whose
`loose_dirs()` still answers ``[]``. `reference` is exactly that today — it writes a file
under ``.charter/vaults/`` through the same private walk, and reports nothing about a
directory that predates it, on either surface. That is a real gap and it is not this
file's: it is filed upstream, and the base class's default is the honest ``[]`` rather than
a guess.
"""

from __future__ import annotations

import argparse
import io
import json
import os
import stat
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from charter import config, doctor
from charter.commands_secrets import cmd_vault_list
from charter.secrets import base, registry
from charter.secrets.plain_file import PlainFileProvider

from tests._isolation import PersonaIso

#: More than 0755, for the reason `test_vault_dir_mode` lists them: 0705 and 0701 grant
#: other without group, 0711 traverse without read, 0730 a group write. A report written
#: against a literal list of bad modes is the failure this suite keeps finding.
LOOSE_MODES = (0o755, 0o750, 0o705, 0o701, 0o711, 0o730, 0o707)


class LooseStateDir(PersonaIso):
    """A plane whose `.charter/` predates the fix: made by hand, at the umask default.

    Not `charter`'s own doing — since #470 charter creates it 0700 — which is exactly why
    this is the case that has to be REPORTED rather than fixed.
    """

    def setUp(self) -> None:
        super().setUp()
        self.sd = Path(config.STATE_DIR)
        self.sd.mkdir(parents=True, exist_ok=True)
        os.chmod(self.sd, 0o755)
        self.vf = self.sd / "vaults" / "devops.json"
        registry.add_vault("devops", "plain-file", {"file": str(self.vf)})
        PlainFileProvider("devops", {"file": str(self.vf)}).set("K", "value-one")
        self.assertEqual(stat.S_IMODE(self.sd.stat().st_mode), 0o755,
                         "precondition: there is something to report")

    def vault_list(self) -> str:
        buf = io.StringIO()
        with redirect_stdout(buf):
            cmd_vault_list(argparse.Namespace())
        return buf.getvalue()


class TheNoteReachesDoctor(LooseStateDir):
    def test_the_green_vaults_line_names_the_directory_and_the_chmod(self) -> None:
        res = doctor.check_vaults()
        rendered = res.render()
        self.assertIn("listed by other accounts", rendered)
        self.assertIn(".charter 755", rendered,
                      f"the line must name the level that is loose, not merely that one "
                      f"is: {rendered!r}")
        self.assertIn("chmod 700", rendered, "and what to type about it")

    def test_it_is_not_a_warn(self) -> None:
        """A loose directory is an operator's decision, not an unreachable vault — and
        this check runs from the SessionStart hook."""
        self.assertEqual(doctor.check_vaults().status, doctor.OK)

    def test_it_reaches_the_json(self) -> None:
        """`doctor --json` is what a script reads, and the note is worth as much there.

        `cmd_doctor` builds each object out of ``r.detail`` (name/status/detail/hint), so
        the note has to be in the DETAIL and not only in the rendered line — a fix that put
        it in `render()` alone would leave `doctor --json` exactly as silent as before.
        """
        res = doctor.check_vaults()
        payload = json.dumps({"name": res.name, "status": res.status,
                              "detail": res.detail, "hint": res.hint})
        self.assertIn("listed by other accounts", payload)

    def test_both_surfaces_render_the_same_list(self) -> None:
        """The property, rather than the prose: `vault list` and `doctor` say the same
        thing because they render the same structured answer, not because two sentences
        were kept in step by hand."""
        note = base.loose_dir_note(registry.provider_for("devops").loose_dirs())
        self.assertTrue(note, "precondition: the provider reports the directory")
        self.assertIn(note, self.vault_list())
        self.assertIn(note, doctor.check_vaults().render())

    def test_every_loose_mode_is_reported_not_just_0755(self) -> None:
        for mode in LOOSE_MODES:
            with self.subTest(mode=oct(mode)):
                os.chmod(self.sd, mode)
                self.assertIn("listed by other accounts", doctor.check_vaults().render(),
                              f"{oct(mode)[-3:]} lets another account in and was not named")

    def test_a_private_state_directory_says_nothing(self) -> None:
        """The complaint has to be caused by the mode. A note that is always there is a
        line people learn to skip, and it takes the rest of the report with it."""
        os.chmod(self.sd, 0o700)
        rendered = doctor.check_vaults().render()
        self.assertNotIn("listed by other accounts", rendered)
        self.assertEqual(doctor.check_vaults().status, doctor.OK)

    def test_one_directory_is_named_once_however_many_vaults_share_it(self) -> None:
        """Several vaults normally sit in one `.charter/`. One directory is one `chmod`,
        and a doctor line that repeats it per vault is the same fact three times."""
        for name in ("second", "third"):
            vf = self.sd / "vaults" / f"{name}.json"
            registry.add_vault(name, "plain-file", {"file": str(vf)})
            PlainFileProvider(name, {"file": str(vf)}).set("K", "v")
        rendered = doctor.check_vaults().render()
        self.assertEqual(rendered.count("listed by other accounts"), 1, rendered)
        self.assertEqual(rendered.count(".charter 755"), 1, rendered)


class TheNoteSurvivesTheOtherPaths(LooseStateDir):
    """`check_vaults` returns from five places. A note that only rides the green one is
    reported in exactly the conditions where nothing else is wrong."""

    def test_an_unreachable_vault_does_not_take_the_note_with_it(self) -> None:
        broken = self.sd / "vaults" / "broken.json"
        registry.add_vault("broken", "plain-file", {"file": str(broken)})
        broken.parent.mkdir(parents=True, exist_ok=True)
        broken.write_text("{ not json")
        res = doctor.check_vaults()
        self.assertEqual(res.status, doctor.WARN, "precondition: a vault must be broken")
        self.assertIn("not reachable", res.render())
        self.assertIn("listed by other accounts", res.render(),
                      "the loose directory did not stop being loose because a different "
                      "vault is also broken")


class ADirectoryCharterMadeIsNotReported(PersonaIso):
    """The pair to the case above, and the one that says #470 landed: on a plane charter
    builds from nothing there is nothing for this note to say."""

    def test_a_plane_charter_built_itself_has_no_note(self) -> None:
        old = os.umask(0o022)
        self.addCleanup(os.umask, old)
        sd = Path(config.STATE_DIR)
        self.assertFalse(sd.exists(), "precondition: charter creates every level here")
        vf = sd / "vaults" / "devops.json"
        registry.add_vault("devops", "plain-file", {"file": str(vf)})
        PlainFileProvider("devops", {"file": str(vf)}).set("K", "value-one")
        self.assertEqual(stat.S_IMODE(sd.stat().st_mode), 0o700,
                         "precondition: #470 — charter chose this mode, not the umask")
        self.assertNotIn("listed by other accounts", doctor.check_vaults().render())


class ProvidersWithoutADirectoryOfCharterS(PersonaIso):
    """A provider that keeps nothing on disk has no directory of charter's to report, and
    the honest answer is an empty list rather than a guess about somebody's backend."""

    def test_the_base_default_is_empty(self) -> None:
        class Nowhere(base.VaultProvider):
            id = "nowhere"

            def get(self, key):  # pragma: no cover - never called
                raise base.SecretNotFound(key)

            def set(self, key, value):  # pragma: no cover - never called
                raise base.VaultError("read-only")

            def delete(self, key):  # pragma: no cover - never called
                raise base.VaultError("read-only")

            def keys(self):
                return []

        self.assertEqual(Nowhere("v", {}).loose_dirs(), [])
        self.assertEqual(base.loose_dir_note([]), "")


if __name__ == "__main__":
    unittest.main()
