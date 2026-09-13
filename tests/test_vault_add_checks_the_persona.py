"""`charter vault add --persona <name>` binds a vault only to a persona this plane defines
(#1057).

`cmd_vault_add` passed the flag straight to `registry.add_vault`, which stores it as the
entry's `persona` field. Measured before, on a plane defining only `devops`: `--persona " "`,
`--persona ../x` and `--persona devosp` each exited 0 with `Vault 'v' registered (provider:
plain-file, persona: …)`, and the registry held the name as given. The value is only a label —
no path is built from it, the plain-file default comes from the vault *name* — so this is not
a traversal. The label is the binding: `persona.vault_of` falls back to the vault tagged with
a persona's name, so a vault tagged `devosp` is found for no persona the plane defines, and
nothing says so until a read fails later, in someone else's session.

**Refused before anything is written, in the words the persona commands already use.** A
whitespace name gets `persona.blank_flag`'s sentence (#1055), a valid name that defines
nothing gets `persona use`'s, create hint included, and a name no persona could have gets
`persona create`'s, with no create hint, because `persona create` would refuse it too.

`vault list` is the other half: a registration made before this, or one whose persona was
removed since, says so on its own row instead of reading as a live binding.
"""
from __future__ import annotations

import io
import shutil
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import commands_secrets, config, tui
from charter.secrets import registry
from tests._isolation import PersonaIso


def _args(name: str, **kw) -> SimpleNamespace:
    return SimpleNamespace(name=name, provider=kw.pop("provider", "plain-file"), file=None,
                           op_vault=None, account=None, persona=kw.pop("persona", None),
                           force=kw.pop("force", False), share=kw.pop("share", False),
                           env=[], token_env=None)


class VaultAddCase(PersonaIso):
    """A plane defining one persona, `devops`, and no vault registry in either half."""

    def setUp(self) -> None:
        super().setUp()
        self.make_persona("devops", role="Devops")

    def add(self, name: str = "v", **kw) -> tuple[int, str]:
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            rc = commands_secrets.cmd_vault_add(_args(name, **kw))
        return rc, err.getvalue()

    def forget_every_registration(self) -> None:
        """Between subtests, so one case's verdict cannot hang on what the last one wrote."""
        for half in (config.VAULTS_REGISTRY, config.SHARED_VAULTS):
            half.unlink(missing_ok=True)

    def assert_nothing_written(self) -> None:
        """Neither half of the registry exists: the refusal came before the first write,
        not after one it then tried to take back."""
        for half in (config.VAULTS_REGISTRY, config.SHARED_VAULTS):
            self.assertFalse(half.exists(), f"{half} was written by a refused `vault add`")


class APersonaNobodyHasIsRefused(VaultAddCase):
    def test_a_name_of_only_whitespace_is_refused_without_a_create_hint(self):
        """Measured before: `--persona " "` exited 0 and stored `" "`. There is no
        `persona create " "` to suggest, so the sentence is `blank_flag`'s, as `persona
        secret` and `recall` say it."""
        for blank, shown in ((" ", " "), ("\t", "\\x09")):
            for share in (False, True):
                with self.subTest(blank=blank, share=share):
                    self.forget_every_registration()
                    rc, err = self.add(persona=blank, share=share)
                    self.assertEqual(rc, 1, err)
                    self.assertIn(f"no persona '{shown}' (a persona name is never only "
                                  "whitespace)", err)
                    self.assertNotIn("persona create", err)
                    self.assert_nothing_written()

    def test_a_name_no_persona_could_have_is_refused_without_a_create_hint(self):
        """`../x` and `Devops` pass no persona's alphabet, so `persona create` would refuse
        either one, and a hint to run it would send the operator into a second refusal."""
        for bad in ("../x", "Devops"):
            for share in (False, True):
                with self.subTest(bad=bad, share=share):
                    self.forget_every_registration()
                    rc, err = self.add(persona=bad, share=share)
                    self.assertEqual(rc, 1, err)
                    self.assertIn(f"invalid persona name '{bad}' (lowercase letters, digits, "
                                  "'.', '_', '-')", err)
                    self.assertNotIn("persona create", err)
                    self.assert_nothing_written()

    def test_a_refused_name_is_shown_on_one_line(self):
        """The name is the operator's, typed into a shell, and the refusal quotes it: a line
        separator in it would write a line of this output charter did not."""
        rc, err = self.add(persona="ops\nforged")
        self.assertEqual(rc, 1, err)
        self.assertIn("invalid persona name 'ops\\x0aforged'", err)
        self.assertEqual(len(err.strip().splitlines()), 1, err)
        self.assert_nothing_written()

    def test_a_valid_name_that_defines_nothing_is_refused_with_the_create_hint(self):
        """The misspelling. `devosp` is a name a persona could have, so the remedy is the one
        `persona use` gives."""
        for share in (False, True):
            with self.subTest(share=share):
                self.forget_every_registration()
                rc, err = self.add(persona="devosp", share=share)
                self.assertEqual(rc, 1, err)
                self.assertIn("no persona 'devosp' (create it: charter persona create devosp)",
                              err)
                self.assert_nothing_written()

    def test_force_does_not_bind_a_registration_to_a_persona_nobody_has(self):
        """`--force` replaces a registration; it is not permission to strand one. The
        existing entry is still the one on disk afterwards."""
        rc, err = self.add(persona="devops")
        self.assertEqual(rc, 0, err)
        rc, err = self.add(persona="devosp", provider="reference", force=True)
        self.assertEqual(rc, 1, err)
        self.assertIn("no persona 'devosp'", err)
        self.assertNotIn("Replaced", err)
        entry = registry.vaults()["v"]
        self.assertEqual((entry["provider"], entry["persona"]), ("plain-file", "devops"))

    def test_the_refusal_is_the_line_the_persona_commands_print(self):
        """Not a sentence of its own. `persona use` owns the create hint and `persona
        create` owns the alphabet, and a third copy is how the same absence comes to be
        described two ways."""
        from charter import commands_persona

        def refusal(cmd, **kw) -> str:
            err = io.StringIO()
            with redirect_stderr(err), redirect_stdout(io.StringIO()):
                self.assertEqual(cmd(SimpleNamespace(**kw)), 1)
            return err.getvalue().strip()

        for owner, name in ((commands_persona.cmd_persona_use, "devosp"),
                            (commands_persona.cmd_persona_create, "../x")):
            with self.subTest(name=name):
                self.forget_every_registration()
                self.assertEqual(self.add(persona=name)[1].strip(),
                                 refusal(owner, name=name, force=False, extends=None))


class APersonaThePlaneDefinesStillBinds(VaultAddCase):
    def test_a_defined_persona_is_bound(self):
        rc, err = self.add(persona="devops")
        self.assertEqual(rc, 0, err)
        self.assertEqual(registry.load_local()["vaults"]["v"]["persona"], "devops")
        self.assertIn("persona: devops", err)

    def test_a_defined_persona_is_bound_in_the_shared_half(self):
        rc, err = self.add(persona="devops", provider="reference", share=True)
        self.assertEqual(rc, 0, err)
        self.assertEqual(registry.load_shared()["vaults"]["v"]["persona"], "devops")

    def test_no_flag_and_an_empty_flag_still_bind_no_persona(self):
        """Empty is no flag, as every reader of `--persona` has always taken it, and `""` is
        how charter's own records spell "no persona"."""
        for i, flag in enumerate((None, "")):
            with self.subTest(flag=flag):
                rc, err = self.add(f"v{i}", persona=flag)
                self.assertEqual(rc, 0, err)
                self.assertFalse(registry.vaults()[f"v{i}"]["persona"])


class VaultListNamesAPersonaThatIsGone(VaultAddCase):
    """A registration bound to a persona this plane does not define: written before the
    refusal above existed, hand-edited, or left behind when the persona was removed."""

    MARK = "(no such persona)"

    def listing(self) -> tuple[int, dict[str, str]]:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            rc = commands_secrets.cmd_vault_list(SimpleNamespace())
        # Split before stripping: `strip_ansi` sanitizes, and a newline is one of the things
        # it turns into a space.
        rows = {}
        for line in out.getvalue().splitlines():
            line = tui.strip_ansi(line)
            rows.setdefault(line.split(" ", 1)[0], line)
        return rc, rows

    def test_a_removed_persona_is_marked_on_its_row_and_the_listing_still_succeeds(self):
        """Measured before: the row read `gone  plain-file  ghost  local  …`, the same as a
        live binding."""
        self.make_persona("ghost", role="Ghost")
        self.assertEqual(self.add("gone", persona="ghost")[0], 0)
        self.assertEqual(self.add("live", persona="devops")[0], 0)
        self.assertEqual(self.add("free")[0], 0)
        shutil.rmtree(config.PERSONAS_DIR / "ghost")
        rc, rows = self.listing()
        self.assertEqual(rc, 0)
        self.assertIn(f"ghost {self.MARK}", rows["gone"])
        self.assertNotIn(self.MARK, rows["live"])
        self.assertNotIn(self.MARK, rows["free"])

    def test_a_label_no_persona_could_have_is_marked_as_the_registry_holds_it(self):
        """Such a label only arrives from a registry file. `tui.pad` keeps the row on one
        line by drawing the separator as a space, which reads as a persona called `a b` that
        the registry does not hold; the escape says what it does hold."""
        registry.add_vault("odd", "plain-file", {"file": "odd.json"}, persona="a\nb")
        rc, rows = self.listing()
        self.assertEqual(rc, 0)
        self.assertNotIn("b", rows, "the label's second half was drawn as a row of its own")
        self.assertIn(f"a\\x0ab {self.MARK}", rows["odd"])


if __name__ == "__main__":
    unittest.main()
