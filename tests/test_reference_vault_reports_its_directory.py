"""A reference vault says where it lives — the directory, the mode, and whether it is there.

#491. A `reference` vault keeps a file under ``.charter/vaults/`` exactly as a plain-file
vault does, through the same private walk, and reported **nothing** about it. On a plane
whose ``.charter/`` predates charter — made by hand, by ``mkdir -p``, or by a charter older
than #470 — a plain-file vault named the loose directory and printed the ``chmod``, and a
reference vault beside it stayed silent, on `charter vault list` and on `charter doctor`
alike::

    devops   plain-file   —   local   1 secret(s), listed by other accounts: .charter 755 (want 700 — chmod 700)
    refs     reference    —   local   no references yet

The values in a reference file are not secrets — that is the whole point of the provider —
but the file lists every item and field this plane reaches, and the DIRECTORY lists the
vault names on the machine, which is exactly the exposure the report exists for.

**The shape of the fix is not "give `reference` a `loose_dirs` too".** That is the second
copy of eight lines, and `VaultProvider.file_path` already records what happens next: it
replaced two byte-identical ``path`` properties, and the reason it had to is that the
second copy is how one provider quietly keeps answering the old way. So the question is
asked once, on the base class, of the one thing that already knows where a vault's file is
— and a provider with no ``file`` (1Password today) makes `file_path` raise and gets the
empty list it always had, with no new attribute for a new provider to forget.

Three findings, one family, and the family is the one this repo keeps filing: **charter
knowing something and not saying it.**

* the directory another account can list — the filed defect;
* the FILE's mode. `plain-file` prints ``perms 644 (want 600)``; `reference` printed
  nothing, though it writes 0600 for a stated reason and a hand-authored or pre-existing
  file inherits the umask;
* the file not being there **at all**, which rendered as ``no references yet`` — the
  wording for an empty vault. A ``--file`` pointing at a path that does not exist is a
  failed read, and *a failed read must never render as a benign state*. `statusline` reads
  the phrase ``not created yet`` to mark such a vault ``◦``, so this was also the one
  provider whose never-written state was invisible there.

Nothing here chmods anything, and that is #331: a vault's ``file`` may name any path on
this machine, so charter tightens what it creates and REPORTS what it did not.
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
from unittest import mock

from charter import config, doctor
from charter.commands_secrets import cmd_vault_list, cmd_vault_verify
from charter.secrets import base, reference, registry
from charter.secrets.plain_file import PlainFileProvider
from charter.secrets.reference import ReferenceProvider

from tests._isolation import PersonaIso

#: The same list `test_doctor_names_a_loose_state_directory` sweeps, and for the reason it
#: gives: 0755 is the mode everybody thinks of, and 0705, 0701, 0711 and 0730 list or
#: traverse just as well. A report written against a literal set of bad values is the
#: defect `base._OTHERS` exists to avoid.
LOOSE_MODES = (0o755, 0o750, 0o705, 0o701, 0o711, 0o730, 0o707)

REF = "op://Eng/deploy/token"


class ReferencePlane(PersonaIso):
    """A plane whose `.charter/` was made by hand at the umask default, holding one
    reference vault — the configuration in #491's reproduction."""

    def setUp(self) -> None:
        super().setUp()
        self.sd = Path(config.STATE_DIR)
        self.sd.mkdir(parents=True, exist_ok=True)
        os.chmod(self.sd, 0o755)
        self.vf = self.sd / "vaults" / "refs.json"
        registry.add_vault("refs", "reference", {"file": str(self.vf)})
        # Both resolvers present, always. Whether `op` happens to be installed on the
        # machine running the suite is not a fact about this fix, and a test that reads it
        # passes for the wrong reason on one of the two kinds of machine. Nothing is
        # spawned: `health` only ever asks `which`.
        self.enterContext(mock.patch.object(reference.shutil, "which",
                                            lambda c: f"/usr/local/bin/{c}"))
        self.assertEqual(stat.S_IMODE(self.sd.stat().st_mode), 0o755,
                         "precondition: there is something to report")

    def provider(self) -> ReferenceProvider:
        return registry.provider_for("refs")

    def write_refs(self, data=None) -> None:
        """The vault file, as charter's own writer leaves it: 0600, parents at 0700."""
        self.provider().set("TOKEN", REF)
        if data is not None:
            self.vf.write_text(json.dumps(data))
            os.chmod(self.vf, 0o600)

    def vault_list(self) -> str:
        buf = io.StringIO()
        with redirect_stdout(buf):
            cmd_vault_list(argparse.Namespace())
        return buf.getvalue()

    def status_row(self) -> str:
        """The one printed row about `refs`, STATUS column and all."""
        rows = [ln for ln in self.vault_list().splitlines() if ln.startswith("refs")]
        self.assertEqual(len(rows), 1, self.vault_list())
        return rows[0]


class TheDirectoryIsNamedOnBothSurfaces(ReferencePlane):
    def test_vault_list_names_the_directory_and_the_chmod(self) -> None:
        self.write_refs()
        row = self.status_row()
        self.assertIn("listed by other accounts", row)
        self.assertIn(".charter 755", row,
                      f"the row must name the level that is loose, not merely that one "
                      f"is: {row!r}")
        self.assertIn("chmod 700", row, "and what to type about it")

    def test_doctor_names_it_too(self) -> None:
        """The half #471 is about: `doctor` keeps `health()`'s boolean and drops its
        string, so a note that lives only in the sentence never reaches this command."""
        self.write_refs()
        rendered = doctor.check_vaults().render()
        self.assertIn("listed by other accounts", rendered)
        self.assertIn(".charter 755", rendered)

    def test_it_reaches_the_json(self) -> None:
        """`doctor --json` is what a script reads. `cmd_doctor` builds its objects out of
        ``r.detail``, so a note that appeared only in `render()` would leave the machine
        readable surface exactly as silent as before."""
        self.write_refs()
        res = doctor.check_vaults()
        payload = json.dumps({"name": res.name, "status": res.status,
                              "detail": res.detail, "hint": res.hint})
        self.assertIn("listed by other accounts", payload)

    def test_both_surfaces_render_the_same_list(self) -> None:
        """The property rather than the prose: the two commands agree because they render
        one structured answer, not because two sentences are kept in step by hand."""
        self.write_refs()
        note = base.loose_dir_note(self.provider().loose_dirs())
        self.assertTrue(note, "precondition: the provider reports the directory")
        self.assertIn(note, self.status_row())
        self.assertIn(note, doctor.check_vaults().render())

    def test_reference_and_plain_file_say_the_same_thing_about_one_directory(self) -> None:
        """The defect was an ASYMMETRY: two vaults in one directory, one reporting it.

        Asserted as "the same rendering", not as two greps for the same words — the point
        of #471's structured answer is that neither provider owns a sentence."""
        self.write_refs()
        pf = self.sd / "vaults" / "devops.json"
        registry.add_vault("devops", "plain-file", {"file": str(pf)})
        PlainFileProvider("devops", {"file": str(pf)}).set("K", "value-one")
        self.assertEqual(base.loose_dir_note(registry.provider_for("refs").loose_dirs()),
                         base.loose_dir_note(registry.provider_for("devops").loose_dirs()),
                         "one directory, one chmod, one sentence")

    def test_every_loose_mode_is_reported_not_just_0755(self) -> None:
        self.write_refs()
        for mode in LOOSE_MODES:
            with self.subTest(mode=oct(mode)):
                os.chmod(self.sd, mode)
                self.assertIn("listed by other accounts", self.status_row(),
                              f"{oct(mode)[-3:]} lets another account in and was not named")

    def test_a_private_directory_says_nothing(self) -> None:
        """The complaint has to be caused by the mode. A note that is always there is a
        line people learn to skip, and it takes the rest of the report with it."""
        self.write_refs()
        os.chmod(self.sd, 0o700)
        os.chmod(self.sd / "vaults", 0o700)
        self.assertNotIn("listed by other accounts", self.status_row())
        self.assertNotIn("listed by other accounts", doctor.check_vaults().render())


class TheNoteRidesEveryBranch(ReferencePlane):
    """`reference.health` returns from five places, and a directory does not stop being
    listable because something else about the vault is also wrong. `check_vaults` records
    the same lesson one level up: a note that rides only the branch where nothing else has
    gone wrong is reported in exactly the conditions nobody is in."""

    def test_before_the_file_exists(self) -> None:
        self.assertFalse(self.vf.exists(), "precondition: nothing written yet")
        _ok, detail = self.provider().health()
        self.assertIn("listed by other accounts", detail)

    def test_with_no_references_in_it(self) -> None:
        self.vf.parent.mkdir(parents=True, exist_ok=True)
        self.vf.write_text("{}\n")
        os.chmod(self.vf, 0o600)
        _ok, detail = self.provider().health()
        self.assertIn("no references yet", detail)
        self.assertIn("listed by other accounts", detail)

    def test_with_references(self) -> None:
        self.write_refs()
        ok, detail = self.provider().health()
        self.assertTrue(ok)
        self.assertIn("1 reference(s)", detail)
        self.assertIn("listed by other accounts", detail)

    def test_when_the_resolver_cli_is_missing(self) -> None:
        self.write_refs()
        with mock.patch.object(reference.shutil, "which", lambda c: None):
            ok, detail = self.provider().health()
        self.assertFalse(ok, "precondition: an unreachable resolver")
        self.assertIn("not on PATH", detail)
        self.assertIn("listed by other accounts", detail,
                      "the directory did not stop being loose because `op` is missing")

    def test_when_the_file_is_not_valid_json(self) -> None:
        self.vf.parent.mkdir(parents=True, exist_ok=True)
        self.vf.write_text("{ not json")
        ok, detail = self.provider().health()
        self.assertFalse(ok, "precondition: unparseable")
        self.assertIn("not valid JSON", detail)
        self.assertIn("listed by other accounts", detail,
                      "the directory did not stop being loose because the file is broken")

    def test_two_missing_clis_are_named_in_a_stable_order(self) -> None:
        """More than one scheme, which the single-scheme cases above cannot see.

        Two things live on this line and neither shows up with one scheme present. The CLI
        names come back in SCHEME order, derived from the sorted `needed` list rather than
        from a set — a set of short strings iterates in hash order and `str` hashing is
        randomised per process, so a health line built from one reorders between runs and
        cannot be diffed. And the search that finds a URI for each scheme is FILTERED: hand
        `_vault_argv` the `op://` entry because it happened to be first and it raises
        "malformed Vault reference" out of a health check.
        """
        self.vf.parent.mkdir(parents=True, exist_ok=True)
        self.vf.write_text(json.dumps({"A": REF, "B": "vault://secret/data/app#TOKEN"}))
        os.chmod(self.vf, 0o600)
        with mock.patch.object(reference.shutil, "which", lambda c: None):
            ok, detail = self.provider().health()
        self.assertFalse(ok)
        self.assertIn("not on PATH: op, vault", detail)

    def test_the_cli_lookup_asks_about_the_scheme_it_was_given(self) -> None:
        """The filter as a property, independent of which entry `dict` happens to yield
        first — `_cli_for` is asked for each scheme against a vault holding both."""
        data = {"A": REF, "B": "vault://secret/data/app#TOKEN"}
        self.assertEqual(reference._cli_for("op", data, {}), "op")
        self.assertEqual(reference._cli_for("vault", data, {}), "vault")

    def test_an_unreachable_reference_vault_does_not_take_the_note_out_of_doctor(self) -> None:
        self.vf.parent.mkdir(parents=True, exist_ok=True)
        self.vf.write_text("{ not json")
        res = doctor.check_vaults()
        self.assertEqual(res.status, doctor.WARN, "precondition: a vault must be broken")
        self.assertIn("not reachable", res.render())
        self.assertIn("listed by other accounts", res.render())


class TheFileSaysItsOwnMode(ReferencePlane):
    """`plain-file` prints ``perms 644 (want 600)``; `reference` printed nothing at all.

    The file is not a secret and it is still worth not publishing — it names every item
    and field this plane reaches. Reported, never repaired: this is a read path, and a
    health check that writes is the defect whatever it writes to (#331)."""

    def _write_at(self, mode: int) -> None:
        self.vf.parent.mkdir(parents=True, exist_ok=True)
        self.vf.write_text(json.dumps({"TOKEN": REF}))
        os.chmod(self.vf, mode)

    def test_a_hand_authored_0644_file_is_named(self) -> None:
        self._write_at(0o644)
        _ok, detail = self.provider().health()
        self.assertIn("perms 644 (want 600)", detail)

    def test_charter_s_own_0600_file_says_nothing(self) -> None:
        self.write_refs()
        self.assertEqual(stat.S_IMODE(self.vf.stat().st_mode), 0o600,
                         "precondition: `_save` chose this mode")
        _ok, detail = self.provider().health()
        self.assertNotIn("perms", detail)

    def test_the_report_does_not_repair_the_mode(self) -> None:
        """`health` runs from the SessionStart hook and a vault's `file` may name any path
        on this machine. Naming it IS the remedy — chmod-ing it is #331."""
        self._write_at(0o644)
        self.provider().health()
        self.assertEqual(stat.S_IMODE(self.vf.stat().st_mode), 0o644,
                         "health() must not write")

    def test_a_clean_vault_says_only_what_is_true(self) -> None:
        """Exact, because the clauses are ASSEMBLED now. A health line built by joining
        three parts has to drop the ones with nothing to say, or every healthy row on every
        plane grows a trailing ``, ,`` that no substring assertion in this file would ever
        notice. Both providers, because both were rewritten to the same join."""
        self.write_refs()
        pf = self.sd / "vaults" / "devops.json"
        registry.add_vault("devops", "plain-file", {"file": str(pf)})
        PlainFileProvider("devops", {"file": str(pf)}).set("K", "v")
        os.chmod(self.sd, 0o700)
        os.chmod(self.sd / "vaults", 0o700)
        self.assertEqual(self.provider().health(), (True, "1 reference(s) via op"))
        self.assertEqual(registry.provider_for("devops").health(), (True, "1 secret(s)"))

    def test_both_providers_use_one_wording(self) -> None:
        self._write_at(0o644)
        pf = self.sd / "vaults" / "devops.json"
        pf.write_text(json.dumps({"K": "v"}))
        os.chmod(pf, 0o644)
        registry.add_vault("devops", "plain-file", {"file": str(pf)})
        self.assertIn(base.mode_note(pf), registry.provider_for("devops").health()[1])
        self.assertIn(base.mode_note(self.vf), self.provider().health()[1])


class AMissingFileIsNotAnEmptyVault(ReferencePlane):
    """*A failed read must never render as a benign state.* ``no references yet`` is what
    an empty vault says; a vault registered against a path that is not there said the same
    thing, and the operator could not tell a typo in ``--file`` from a vault nobody had
    filled in."""

    def test_a_file_that_does_not_exist_says_so_and_names_the_path(self) -> None:
        self.assertFalse(self.vf.exists(), "precondition")
        ok, detail = self.provider().health()
        self.assertTrue(ok, "not created yet is not an unreachable vault")
        self.assertIn("not created yet", detail)
        self.assertIn("refs.json", detail, "and the path it looked at")
        self.assertNotIn("no references yet", detail)

    def test_an_existing_empty_file_still_says_no_references_yet(self) -> None:
        """The other half. Both states are legitimate and they are different facts."""
        self.vf.parent.mkdir(parents=True, exist_ok=True)
        self.vf.write_text("{}\n")
        os.chmod(self.vf, 0o600)
        ok, detail = self.provider().health()
        self.assertTrue(ok)
        self.assertIn("no references yet", detail)
        self.assertNotIn("not created yet", detail)

    def test_the_two_providers_agree_about_a_file_that_is_not_there(self) -> None:
        """`statusline._vault_dot` marks a vault ``◦`` by matching this exact phrase, so a
        reference vault was the one provider whose never-written state was invisible there
        as well. Asserted through the phrase the reader keys on, because that IS the
        contract between the two modules."""
        pf = self.sd / "vaults" / "devops.json"
        registry.add_vault("devops", "plain-file", {"file": str(pf)})
        self.assertIn("not created yet", registry.provider_for("devops").health()[1])
        self.assertIn("not created yet", self.provider().health()[1])

    def test_the_statusline_marks_it(self) -> None:
        from charter import statusline
        self.assertIn("◦", statusline._vault_dot("refs"))


class AReferenceThatResolvesToNothing(ReferencePlane):
    """The fourth instance on this line, and the one that took `doctor` with it.

    `set` refuses anything that is not a supported URI — but entries arrive in this file by
    hand and, for this provider above all, **by commit**: the argument for reference vaults
    is that a team commits the wiring and a fresh clone inherits it. `_load` checks that
    the document is an object and nothing about its values.
    """

    def _write_raw(self, data) -> None:
        self.vf.parent.mkdir(parents=True, exist_ok=True)
        self.vf.write_text(json.dumps(data))
        os.chmod(self.vf, 0o600)

    def test_a_vault_of_only_unsupported_entries_is_not_healthy(self) -> None:
        """It read ``1 reference(s) via `` — a green line whose sentence stops after the
        word "via" because there was no scheme to name."""
        self._write_raw({"K": "https://example.test/x"})
        ok, detail = self.provider().health()
        self.assertFalse(ok)
        self.assertIn("not a supported URI", detail)
        self.assertFalse(detail.rstrip().endswith("via"), detail)

    def test_a_dead_entry_beside_a_live_one_is_named(self) -> None:
        """The sharper half: ``2 reference(s) via op`` counted the dead one and named only
        the scheme that works, so the count said two and one of them could never resolve."""
        self._write_raw({"K": "https://example.test/x", "T": REF})
        ok, detail = self.provider().health()
        self.assertFalse(ok)
        self.assertIn("2 reference(s)", detail)
        self.assertIn("1 not a supported URI", detail)

    def test_it_names_the_command_that_names_the_keys(self) -> None:
        """#371: a report is worth its row only if it says what to do next. The key itself
        stays off this row — it is a value out of a committed JSON object landing in a
        table — and `charter vault verify` already prints ``<key>: <error>`` per failure."""
        self._write_raw({"K": "https://example.test/x"})
        _ok, detail = self.provider().health()
        self.assertIn("charter vault verify", detail)

        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = cmd_vault_verify(argparse.Namespace(name="refs"))
        self.assertEqual(rc, 1, "a dead reference is a non-zero verify")
        self.assertIn("K", buf.getvalue(), "and the command named in the row names the key")

    def test_a_non_string_value_does_not_crash_the_report(self) -> None:
        """`charter doctor` runs from the SessionStart hook and catches `VaultError` and
        nothing else. A committed ``{"K": 123}`` reached `urlsplit` and raised
        ``AttributeError`` out of `health`, so the command whose job is reporting on this
        file was the thing this file could take down."""
        self._write_raw({"K": 123})
        ok, detail = self.provider().health()
        self.assertFalse(ok)
        self.assertIn("not a supported URI", detail)
        self.assertIn("refs", self.status_row())
        self.assertEqual(doctor.check_vaults().status, doctor.WARN)

    def test_scheme_of_is_total_over_anything_json_holds(self) -> None:
        for value in (None, 123, 1.5, True, [], {}, ["op://v/i/f"], {"op": "//v/i/f"}):
            with self.subTest(value=value):
                self.assertIsNone(reference.scheme_of(value))
        self.assertEqual(reference.scheme_of(REF), "op")

    def test_the_note_rides_this_branch_too(self) -> None:
        self._write_raw({"K": "https://example.test/x"})
        _ok, detail = self.provider().health()
        self.assertIn("listed by other accounts", detail)


class OneImplementationForEveryFileBackedProvider(PersonaIso):
    """Why this is on the base class and not copied per provider.

    `VaultProvider.file_path` already replaced two byte-identical ``path`` properties and
    says why: the second copy is how one provider quietly keeps answering the old way.
    `loose_dirs` was that same mistake one method over, and #491 is the bill for it."""

    def test_it_is_not_overridden_anywhere(self) -> None:
        """The structural claim. A provider that re-declares `loose_dirs` has reopened the
        gap this closed, so the test is about the class dict rather than about output."""
        for cls in (PlainFileProvider, ReferenceProvider):
            with self.subTest(provider=cls.id):
                self.assertNotIn("loose_dirs", vars(cls),
                                 f"{cls.__name__} answers `loose_dirs` from the base class; "
                                 f"a copy here is what #491 was filed about")

    def test_a_provider_with_no_file_answers_empty(self) -> None:
        """1Password keeps nothing of charter's on this disk, so there is no directory of
        charter's to report and the honest answer is ``[]`` — not a guess about somebody
        else's backend, and not a crash from `file_path` raising."""
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

    def test_the_1password_provider_still_answers_empty(self) -> None:
        from charter.secrets.onepassword import OnePasswordProvider
        prov = OnePasswordProvider("op", {"op_vault": "Eng", "op_item": "charter"})
        self.assertEqual(prov.loose_dirs(), [])
        self.assertEqual(prov.health.__qualname__.split(".")[0], "OnePasswordProvider",
                         "precondition: this provider has a health line of its own")

    def test_a_reference_vault_with_no_file_configured_does_not_raise(self) -> None:
        """A health line that can throw is a `doctor` that cannot run, and `doctor` calls
        this from the SessionStart hook."""
        prov = ReferenceProvider("broken", {})
        self.assertEqual(prov.loose_dirs(), [])
        ok, detail = prov.health()
        self.assertFalse(ok)
        self.assertIn("no 'file' configured", detail)

    def test_a_file_outside_the_plane_reports_only_its_own_directory(self) -> None:
        """`loose_dirs`' documented bound, now that a second provider reaches it: a vault
        deliberately kept outside the plane must not drag ``/Users`` or ``/`` into the
        report — those are 0755 on every machine and are nobody's defect."""
        outside = Path(self.tmp) / "elsewhere"
        outside.mkdir(parents=True, exist_ok=True)
        os.chmod(outside, 0o755)
        prov = ReferenceProvider("out", {"file": str(outside / "refs.json")})
        found = [p for p, _m in prov.loose_dirs()]
        self.assertEqual([Path(p).resolve() for p in found], [outside.resolve()])


if __name__ == "__main__":
    unittest.main()
