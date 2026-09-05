"""A plane's format version, and the refusal that is the only thing it buys (#913).

`instance.SCHEMA` is stamped into `charter.toml` by `charter init` and read on the way into
every command by `config.derive`. What #913 added is not the number — that existed — but
the two properties that make it worth having:

* **a refusal.** A plane declaring a format version this charter cannot place is not
  operated on. `config.PLANE_REFUSAL` records it, `cli.main` declines the command,
  `doctor`'s `schema` row reports it. A version nobody refuses on is decoration.
* **a definite meaning for an absent stamp.** :data:`instance.UNSTAMPED` is 1, not
  "whatever this charter is" — see the class that pins it for why the difference is the
  whole feature.

And the relationship to the other number, which is the thing two version numbers most
easily get wrong: `instance.SCHEMA` refuses, `workspace.STRUCTURE_VERSION` repairs, and
nothing compares them. `ThePlaneVersionAndTheStructureVersionAreNeverCompared` is the pin.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr
from unittest import mock

from charter import cli, commands, commands_update, config, doctor, instance, workspace
from charter.frame import state as frame_state

from tests._isolation import PersonaIso


class PlaneIso(PersonaIso):
    """A tmp plane whose `charter.toml` this case writes, re-derived after every write."""

    def declare(self, body: str) -> None:
        (self.tmp / "charter.toml").write_text(body)
        config.use(self.tmp)

    def declare_schema(self, value: str) -> None:
        self.declare(f"schema = {value}\n")


class TheAbsentStampMeansVersionOne(PlaneIso):
    """A plane created before planes declared a version is a version-1 plane.

    **This is the case that has to move `SCHEMA` to prove anything.** `SCHEMA` is 1 today,
    so "absent means 1" and the reading it replaced — "absent means whatever this charter
    is" — agree on every plane in existence. They part company exactly once: on the release
    that bumps the number, which is the first time the feature is ever needed. A test that
    read an unstamped plane against the shipped `SCHEMA` would pass under either rule and
    would therefore be pinning nothing at all.
    """

    def test_an_unstamped_plane_reads_as_one_however_far_charter_has_moved(self):
        with mock.patch.object(instance, "SCHEMA", 7):
            self.assertEqual(instance.plane_version({}), 1)

    def test_absent_is_not_read_as_the_current_version(self):
        """The negative half, stated on its own: a bumped charter must not adopt an
        unstamped plane as its own. Doing so reads a version-1 layout with version-7 rules
        — the guess this whole number exists to prevent, arrived at through the number."""
        with mock.patch.object(instance, "SCHEMA", 7):
            self.assertNotEqual(instance.plane_version({}), instance.SCHEMA)

    def test_the_constant_says_one_and_the_reader_uses_it(self):
        self.assertEqual(instance.UNSTAMPED, 1)
        with mock.patch.object(instance, "UNSTAMPED", 4):
            self.assertEqual(instance.plane_version({}), 4)

    def test_an_unstamped_plane_is_loaded_rather_than_refused(self):
        """Definite is not the same as refused. Every plane on disk older than #913 is
        unstamped, and refusing them would be a charter that cannot open its own planes."""
        self.declare('[[forge]]\nkind = "gitlab"\nowner = "acme"\n')
        self.assertIsNone(config.PLANE_REFUSAL)
        self.assertEqual(instance.load(self.tmp)["forge"][0]["owner"], "acme")


class TheBoundaryBetweenUnderstoodAndRefused(PlaneIso):
    """`found > SCHEMA`, and both sides of that comparison get a case at the boundary.

    A version comparison is what a shift-boundary mutation attacks: `>` re-spelled `>=`
    refuses every plane there is, and `>=` re-spelled `>` accepts one version from the
    future. Neither is catchable by a case that tests `SCHEMA - 5` against `SCHEMA + 5`, so
    the three cases below sit on `SCHEMA - 1`, `SCHEMA` and `SCHEMA + 1`.
    """

    def test_exactly_the_version_this_charter_understands_is_accepted(self):
        self.declare_schema(str(instance.SCHEMA))
        self.assertIsNone(config.PLANE_REFUSAL)
        self.assertEqual(instance.load(self.tmp)["schema"], instance.SCHEMA)

    def test_one_version_older_is_accepted(self):
        with mock.patch.object(instance, "SCHEMA", 7):
            self.declare_schema("6")
            self.assertIsNone(config.PLANE_REFUSAL)

    def test_one_version_newer_is_refused(self):
        self.declare_schema(str(instance.SCHEMA + 1))
        self.assertIsNotNone(config.PLANE_REFUSAL)
        with self.assertRaises(instance.SchemaTooNew):
            instance.load(self.tmp)

    def test_the_refusal_names_both_numbers(self):
        """The operator's next question is "how far ahead is it", and the answer is two
        integers. A message carrying only one of them cannot be acted on."""
        self.declare_schema(str(instance.SCHEMA + 1))
        self.assertIn(str(instance.SCHEMA + 1), config.PLANE_REFUSAL)
        self.assertIn(str(instance.SCHEMA), config.PLANE_REFUSAL)


class AVersionCharterCannotCompareIsAlsoRefused(PlaneIso):
    """`schema = "2"` is not a version this charter can place, so it is not operated on.

    It used to fall straight through `isinstance(found, int)` and be treated as understood
    — the plane most likely to have been written by something charter has never seen,
    silently accepted. `schema = true` is the sharp one: `isinstance(True, int)` is `True`
    in Python, so without the explicit `bool` exclusion it compares as 1 and reads as
    current.
    """

    def test_a_string_version_is_refused(self):
        self.declare_schema('"2"')
        self.assertIsNone(instance.plane_version({"schema": "2"}))
        self.assertIsNotNone(config.PLANE_REFUSAL)

    def test_a_fractional_version_is_refused(self):
        self.declare_schema("1.5")
        self.assertIsNone(instance.plane_version({"schema": 1.5}))
        self.assertIsNotNone(config.PLANE_REFUSAL)

    def test_a_boolean_version_is_refused_rather_than_read_as_one(self):
        self.declare_schema("true")
        self.assertIsNone(instance.plane_version({"schema": True}))
        self.assertIsNotNone(config.PLANE_REFUSAL)

    def test_the_refusal_is_the_same_kind_the_cli_gate_reads(self):
        """One exception family, so `config.derive` and `cli.main` need one branch and not
        two — and `SchemaTooNew` keeps working for everything that already caught it."""
        self.assertTrue(issubclass(instance.SchemaTooNew, instance.PlaneFormatUnknown))
        self.declare_schema('"2"')
        with self.assertRaises(instance.PlaneFormatUnknown):
            instance.load(self.tmp)


class TheRefusalIsRecordedApartFromAParseError(PlaneIso):
    """Two failures of one file, and only one of them stops charter.

    Malformed TOML is a typo the operator fixes, and carrying on with empty defaults while
    `doctor` names it is the trade charter already made. A plane from the future is not
    that: every default charter would carry on with is a guess about a layout it has been
    told it does not understand.
    """

    def test_a_too_new_plane_sets_both_names(self):
        """`CONFIG_ERROR` stays set as well, deliberately: `doctor`'s `charter.toml` row
        and `channel.channel`'s documented fallback both key off it."""
        self.declare_schema(str(instance.SCHEMA + 1))
        self.assertIsNotNone(config.PLANE_REFUSAL)
        self.assertEqual(config.CONFIG_ERROR, config.PLANE_REFUSAL)

    def test_malformed_toml_is_an_error_and_not_a_refusal(self):
        self.declare("this is not = valid = toml\n")
        self.assertIsNotNone(config.CONFIG_ERROR)
        self.assertIsNone(config.PLANE_REFUSAL)

    def test_a_healthy_plane_sets_neither(self):
        self.declare_schema(str(instance.SCHEMA))
        self.assertIsNone(config.CONFIG_ERROR)
        self.assertIsNone(config.PLANE_REFUSAL)

    def test_the_refusal_is_one_of_the_settings_the_test_harness_isolates(self):
        """`config.DERIVED` is the source of truth for what `config.use` swaps. A setting
        absent from it leaks the developer's own plane into every case that reads it."""
        self.assertIn("PLANE_REFUSAL", config.DERIVED)


class TheCliDeclinesRatherThanGuesses(PlaneIso):
    """The refusal, where it is worth something: charter does not run the command.

    A warning would leave the command running against a layout charter cannot place, which
    is the state #913 exists to end — and is what the code did before it, since
    `config.derive` swallowed the exception and handed every command an empty config.
    """

    def setUp(self) -> None:
        super().setUp()
        self.ran: list[str] = []
        for mod, name in ((commands, "cmd_status"), (commands, "cmd_doctor"),
                          (commands, "cmd_version"), (commands, "cmd_version_check"),
                          (commands_update, "cmd_update")):
            self.enterContext(mock.patch.object(
                mod, name, lambda args, _n=name: self.ran.append(_n) or 0))

    def run_cli(self, argv: list[str]) -> tuple[int, str]:
        err = io.StringIO()
        with redirect_stderr(err):
            rc = cli.main(argv)
        return rc, err.getvalue()

    def test_an_ordinary_command_does_not_run_on_a_plane_charter_refuses(self):
        self.declare_schema(str(instance.SCHEMA + 1))
        rc, err = self.run_cli(["status"])
        self.assertEqual(rc, 1)
        self.assertEqual(self.ran, [])
        self.assertIn(str(instance.SCHEMA + 1), err)

    def test_the_same_command_runs_on_a_plane_charter_understands(self):
        """The precondition. A gate that refused everything would pass the case above
        while making charter useless, and the two are indistinguishable without this."""
        self.declare_schema(str(instance.SCHEMA))
        rc, _ = self.run_cli(["status"])
        self.assertEqual(rc, 0)
        self.assertEqual(self.ran, ["cmd_status"])

    def test_doctor_still_runs_because_it_is_where_the_refusal_is_reported(self):
        self.declare_schema(str(instance.SCHEMA + 1))
        rc, _ = self.run_cli(["doctor"])
        self.assertEqual(rc, 0)
        self.assertEqual(self.ran, ["cmd_doctor"])

    def test_update_still_runs_because_it_is_the_only_way_out(self):
        """Nothing but a newer charter can understand a newer plane. Refusing the remedy
        leaves an operator with a hand edit of `charter.toml` as their only move."""
        self.declare_schema(str(instance.SCHEMA + 1))
        rc, _ = self.run_cli(["update"])
        self.assertEqual(rc, 0)
        self.assertEqual(self.ran, ["cmd_update"])

    def test_every_exempt_command_is_about_charter_and_not_about_the_plane(self):
        """The exemptions, one case, by name. Each runs; each is a question about which
        charter this is or how to get a newer one — never a read of the plane's contents."""
        self.assertEqual(set(cli._DESPITE_REFUSAL),
                         {"doctor", "update", "version", "_version-check"})
        for argv, handler in ((["version"], "cmd_version"),
                              (["_version-check"], "cmd_version_check")):
            with self.subTest(argv=argv):
                self.declare_schema(str(instance.SCHEMA + 1))
                self.ran.clear()
                self.assertEqual(self.run_cli(argv)[0], 0)
                self.assertEqual(self.ran, [handler])

    def test_a_command_that_writes_into_the_plane_is_not_exempt(self):
        """`init` and `reinit` are the tempting exemptions and the wrong ones: writing into
        a layout charter has been told it does not understand is the guess at its most
        damaging."""
        self.assertNotIn("init", cli._DESPITE_REFUSAL)
        self.assertNotIn("reinit", cli._DESPITE_REFUSAL)

    def test_the_hook_dispatcher_is_not_exempt_either(self):
        """A hook on a refused plane already covers nothing — `known_forges_report` opens
        `charter.toml` through `instance.load`, gets the refusal and returns an empty forge
        set, so the PreToolUse guard looks present and denies nothing. Refusing out loud
        turns a silent fail-open into a visible one, at the cost of the session's briefing
        and never its turn: the gate returns 1, not `hooks.DENY_EXIT`."""
        self.assertNotIn("hook", cli._DESPITE_REFUSAL)
        self.declare_schema(str(instance.SCHEMA + 1))
        with mock.patch("charter.hooks.dispatch") as dispatched:
            rc, _ = self.run_cli(["hook", "sessionstart"])
        self.assertEqual(rc, 1)
        dispatched.assert_not_called()

    def test_the_gate_survives_a_subcommand_that_owns_the_command_dest(self):
        """`charter secret exec` gives its own positional the dest `command` (`_sa_exec`),
        so argparse overwrites the subparser name with the child command line and
        `args.command` is a LIST there. A gate reading it would raise `TypeError: unhashable
        type: 'list'` out of `not in` — a crash report filed against charter, on the one
        plane charter had already decided not to touch. `argv[0]` is the typed token."""
        self.declare_schema(str(instance.SCHEMA + 1))
        rc, err = self.run_cli(["secret", "exec", "myvault", "--", "echo", "hi"])
        self.assertEqual(rc, 1)
        self.assertIn("Nothing was run", err)

    def test_a_bare_charter_does_not_launch_a_harness_on_a_refused_plane(self):
        """`_bare_launch` rewrites bare `charter` into `[<default harness>]` before the
        gate, so the rewritten token is what gets refused. Opening a frame over a plane
        charter cannot place is the most expensive guess of the lot."""
        self.declare(f"schema = {instance.SCHEMA + 1}\n[harness]\ndefault = \"claude\"\n")
        with mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch.object(cli.commands_frame, "cmd_launch") as launched:
            rc, _ = self.run_cli([])
        self.assertEqual(rc, 1)
        launched.assert_not_called()


class DoctorReportsTheRefusalOnTheRowNamedAfterTheNumber(PlaneIso):
    """`doctor`'s `schema` row said `up to date (schema 1)` over a plane declaring 99.

    `instance.drift` counts baseline directories and a plane from the future has all three,
    so the one row an operator would read to find out which version was declared reported
    the opposite of the truth.
    """

    def setUp(self) -> None:
        super().setUp()
        for name in instance.BASELINE_DIRS:
            (self.tmp / name).mkdir(parents=True, exist_ok=True)

    def test_the_schema_row_fails_and_names_the_declared_version(self):
        self.declare_schema(str(instance.SCHEMA + 1))
        row = doctor.check_control_plane_schema()
        self.assertEqual(row.status, doctor.FAIL)
        self.assertIn(str(instance.SCHEMA + 1), row.detail)

    def test_the_hint_says_charter_stopped_and_what_moves_it(self):
        self.declare_schema(str(instance.SCHEMA + 1))
        self.assertIn("charter update", doctor.check_control_plane_schema().hint)

    def test_a_plane_charter_understands_still_reports_up_to_date(self):
        self.declare_schema(str(instance.SCHEMA))
        row = doctor.check_control_plane_schema()
        self.assertEqual(row.status, doctor.OK)
        self.assertIn("up to date", row.detail)

    def test_the_charter_toml_row_stops_promising_a_fallback_that_does_not_happen(self):
        """Its hint says charter falls back to empty defaults, which is true of malformed
        TOML and false of a refusal — nothing carries on to fall back."""
        self.declare_schema(str(instance.SCHEMA + 1))
        refused = doctor.check_control_plane_config()
        self.assertEqual(refused.status, doctor.FAIL)
        self.assertNotIn("Falling back", refused.hint)
        self.declare("this is not = valid = toml\n")
        broken = doctor.check_control_plane_config()
        self.assertEqual(broken.status, doctor.FAIL)
        self.assertIn("Falling back", broken.hint)

    def test_the_row_keeps_its_name(self):
        """`doctor._FIXED_CHECK_NAMES` is pinned by equality against what `run_all`
        produces. The refusal reuses the `schema` row rather than adding a second one —
        two rows about one number are two places for an operator to read a different
        answer."""
        self.assertIn("schema", doctor._FIXED_CHECK_NAMES)
        self.declare_schema(str(instance.SCHEMA + 1))
        self.assertEqual(doctor.check_control_plane_schema().name, "schema")


class ThePlaneVersionAndTheStructureVersionAreNeverCompared(PlaneIso):
    """Two version numbers, and the rule that keeps them from disagreeing.

    They are nested — a workspace lives inside a plane — and only the outer one refuses:

    * `instance.SCHEMA` is the REFUSAL number. Yes or no; nothing heals a no but a newer
      charter.
    * `workspace.STRUCTURE_VERSION` is the REPAIR number. Stale is flagged and healed by
      `charter workspace reinit`, and a workspace an older charter can still read is
      exactly what makes that repair additive.

    So a workspace-layout change an older charter SURVIVES is, by definition, not a change
    that requires refusal, and it moves `STRUCTURE_VERSION` alone. Two numbers can only
    come to disagree if something compares them, and the four cases below are what says
    nothing does.
    """

    def test_a_stale_workspace_is_repaired_and_never_refused(self):
        self.declare_schema(str(instance.SCHEMA))
        workspace.scaffold("ws")
        (workspace.workspace_dir("ws") / ".charter-structure").write_text(
            str(workspace.STRUCTURE_VERSION - 1))
        self.assertTrue(workspace.needs_reinit("ws"))
        self.assertIsNone(config.PLANE_REFUSAL)

    def test_a_refused_plane_is_refused_however_current_its_workspaces_are(self):
        self.declare_schema(str(instance.SCHEMA))
        workspace.scaffold("ws")
        self.assertFalse(workspace.needs_reinit("ws"))
        self.declare_schema(str(instance.SCHEMA + 1))
        self.assertIsNotNone(config.PLANE_REFUSAL)

    def test_moving_the_plane_version_does_not_move_the_workspace_verdict(self):
        """The refusal number is not an input to the repair question. `structure_status`
        must answer identically with `SCHEMA` anywhere at all."""
        self.declare_schema(str(instance.SCHEMA))
        workspace.scaffold("ws")
        before = workspace.structure_status("ws")
        with mock.patch.object(instance, "SCHEMA", 99):
            self.assertEqual(workspace.structure_status("ws"), before)

    def test_moving_the_workspace_version_does_not_move_the_plane_verdict(self):
        """And the repair number is not an input to the refusal. The plane's answer is a
        fact about `charter.toml` alone."""
        self.declare_schema(str(instance.SCHEMA))
        with mock.patch.object(workspace, "STRUCTURE_VERSION", 99):
            config.use(self.tmp)
            self.assertIsNone(config.PLANE_REFUSAL)
            self.assertEqual(instance.plane_version(instance.load(self.tmp)),
                             instance.SCHEMA)

    def test_the_two_numbers_hold_different_values_today(self):
        """5 and 1. Not a requirement — they may collide by coincidence — but a number
        that tracked the other would be a second spelling of one fact, and the day they
        were made equal by hand is the day somebody starts comparing them."""
        self.assertNotEqual(workspace.STRUCTURE_VERSION, instance.SCHEMA)


class FrameStateCarriesNoFormatPromise(PlaneIso):
    """#913's third question, answered "private" rather than "versioned".

    A format version buys the ability to refuse, and refusal is worth something only when
    the writer and the reader can be different charters. A plane's files are committed,
    shared and outlive any install; nothing under `.charter/frame/` is. It is written by a
    launcher, read by that frame's own panels, and any residue of another charter is
    reaped rather than read — so a version stamped there could never fire, which is the
    definition of decoration.
    """

    def test_the_promise_is_stated_as_a_value_rather_than_left_in_prose(self):
        self.assertIn("no format version", frame_state.NO_FORMAT_PROMISE)
        self.assertIn("no promise", frame_state.NO_FORMAT_PROMISE)

    def test_the_promise_names_the_interface_it_points_a_reader_at(self):
        """"Private" with nothing offered in its place is a refusal to answer. The
        commands are the interface — git's own split, and the one this issue rests on."""
        self.assertIn("charter frame", frame_state.NO_FORMAT_PROMISE)

    def test_nothing_under_frame_stamps_or_reads_a_format_version(self):
        """The tripwire, and the reason the claim is worth more than a comment. Frame state
        carrying a version would mean somebody outside charter had come to depend on its
        shape — at which point the promise above has changed and this test is where to say
        so, rather than a version quietly appearing beside one that already exists.

        **Tokenised, not grepped.** Comments and docstrings under `charter/frame/` name
        `instance.SCHEMA` freely — the constant above does it at length, to explain why the
        frame does not carry one — and a substring search would call every one of those an
        offence. `tokenize` keeps only NAME tokens, so this reads what the module *does*.
        """
        import tokenize
        from pathlib import Path

        forbidden = {"SCHEMA", "STRUCTURE_VERSION", "UNSTAMPED"}
        offenders = []
        for p in sorted(Path(frame_state.__file__).resolve().parent.glob("*.py")):
            with tokenize.open(p) as fh:
                names = {t.string for t in tokenize.generate_tokens(fh.readline)
                         if t.type == tokenize.NAME}
            if names & forbidden:
                offenders.append(p.name)
        self.assertEqual(offenders, [], (
            "a module under charter/frame/ now names a format version in code. "
            "`.charter/frame/` is declared private and unversioned "
            "(frame.state.NO_FORMAT_PROMISE); if that changed, change the promise first."))

    def test_the_tripwire_can_actually_fire(self):
        """A source scan that matched nothing would pass the case above forever. This runs
        the same scan over a module that DOES carry the number — `charter/instance.py`,
        where the plane's own version lives — so "no offenders" is a finding rather than a
        scanner that lost its way to the files."""
        import tokenize
        from pathlib import Path

        with tokenize.open(Path(instance.__file__).resolve()) as fh:
            names = {t.string for t in tokenize.generate_tokens(fh.readline)
                     if t.type == tokenize.NAME}
        self.assertIn("SCHEMA", names)


if __name__ == "__main__":
    unittest.main()
