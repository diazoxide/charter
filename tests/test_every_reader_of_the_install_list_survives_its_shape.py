"""Every reader of Claude Code's install list survives every shape of it, and reads as reaching
only a list that matches Claude Code 2.1.272's schema exactly.

`<config folder>/plugins/installed_plugins.json` is Claude Code's file, and a chat can write it.
0.62.0 read it in three places that assumed the shape Claude Code writes, and a list in any
other shape raised out of all of them:

* `charter doctor` printed a traceback and no rows — `_checks()` has no per-check guard;
* the SessionStart preflight printed "charter preflight failed" with that traceback at every
  session start;
* `charter init` and `charter reinit` raised in `_ensure_guard_hook`.

**The schema** is read from Claude Code 2.1.272's schema — the version-2 install list's zod
objects in that binary — and measured against the same binary in throwaway folders. A list that
does not match it is refused whole: Claude Code logs "Failed to load installed_plugins.json" and
loads no plugin. The refusing checks: `version` is the literal 2; `plugins` is an object whose
keys are `plugin@marketplace` (`[A-Za-z0-9][-A-Za-z0-9._]*` on each side) and whose values are
arrays; every record is an object with a `scope` among `managed`, `user`, `project`, `local`
and a string `installPath`; `projectPath`, `version`, `installedAt`, `lastUpdated`,
`gitCommitSha` and `resolvedVersion` are strings when present, and `auto` a boolean. Unknown
keys, and `claudeaiPluginId`, `archiveSha256`, `sourceCommand`, `sourceProducerPath` and
`previousProducerPaths` of any type, the schema drops (`.catch(void 0)`) and loads — measured.

**And it is parsed as `JSON.parse` parses it.** Python's `json` accepts `NaN`, `Infinity` and
`-Infinity`; `JSON.parse`, which Claude Code reads the list with, refuses them. A list or a
settings file holding one is not a file Claude Code reads (round 4 of the review).

Any other list is "could not tell", a version-1 list included: 2.1.272 migrates one, and loads
it from a path it computes rather than the `installPath` in the file, which is not a reading
charter keeps. So `doctor` warns, and `init`/`reinit` write the guard hook as though no plugin
dispatched it — the safe direction: a guard declared twice runs twice, which is harmless and
reported; a guard declared nowhere is a hole. A newer Claude Code that changes the schema gets
the same answer until charter follows it.
"""

from __future__ import annotations

import io
import json
import os
import unittest
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, doctor, guardseen
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN
PID = "charter@charter"
REPO = Path(__file__).resolve().parents[1]

#: Where the schema these tests hold charter to comes from. `docs/install.md` names it too, so a
#: reader can tell which Claude Code the rows were written against.
SCHEMA_SOURCE = "read from Claude Code 2.1.272's schema"

#: The fields 2.1.272's version-2 record types and refuses on a wrong type, besides `scope` and
#: `installPath`: optional strings, and one optional boolean. Hand-spelled from that schema.
OPTIONAL_STRINGS = ("projectPath", "version", "installedAt", "lastUpdated", "gitCommitSha",
                    "resolvedVersion")
OPTIONAL_BOOLEAN = "auto"

#: Hand-spelled, never imported from the module under test.
_PRETOOLUSE = {"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
    {"type": "command", "command": "charter hook pretooluse"}]}]}}

_UNWIRED = "pretooluse is not wired"


def preflight() -> str:
    """What the SessionStart hook runs: `charter doctor --preflight`, streamed, as text."""
    out = io.StringIO()
    with redirect_stdout(out):
        commands.cmd_doctor(SimpleNamespace(fix=False, preflight=True, json=False))
    return out.getvalue()


def guard_line(text: str) -> str:
    return next((line for line in text.splitlines() if "plane-root guard" in line), text)


class ListShapeCase(PersonaIso):
    """A plane enabling charter's plugin, whose files dispatch the guard, and a session there."""

    def setUp(self) -> None:
        super().setUp()
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        self.home = self.tmp / "home"
        (self.home / ".claude").mkdir(parents=True)
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.home)}))
        for var in ("CLAUDE_CONFIG_DIR", "CLAUDE_PLUGIN_ROOT", "CLAUDE_CODE_PLUGIN_CACHE_DIR"):
            os.environ.pop(var, None)
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)
        self.assertTrue(doctor.session_is_the_plane())
        self.plane = Path(config.ROOT).resolve()
        self.plugin = self.tmp / "plugin-cache" / "charter"
        self.write(self.plugin / "hooks" / "hooks.json", _PRETOOLUSE)

    def write(self, path: Path, doc) -> None:
        """*doc* as text, or as JSON — Python's, which writes `NaN` and `Infinity` as such."""
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(doc if isinstance(doc, str) else json.dumps(doc))

    @property
    def manifest(self) -> Path:
        return self.home / ".claude" / "plugins" / "installed_plugins.json"

    @property
    def settings(self) -> Path:
        return config.ROOT / ".claude" / "settings.json"

    def enable(self, pid: str = PID) -> None:
        """Reset the plane's settings to enabling *pid* and nothing else."""
        self.write(self.settings, {"enabledPlugins": {pid: True}})

    def valid(self, project=None) -> dict:
        return {"scope": "project", "projectPath": str(project or self.plane),
                "installPath": str(self.plugin)}

    def full(self) -> dict:
        """A record with every field the schema types, each of the type it requires."""
        return {**self.valid(), "version": "0.62.0", "installedAt": "2026-09-15T13:04:28.952Z",
                "lastUpdated": "2026-09-15T13:04:28.952Z",
                "gitCommitSha": "5ad755d681b7e74b9bdf137b2f3270b282f34e1e",
                "resolvedVersion": "0.62.0", "auto": True}

    def listed(self, *records, **others) -> dict:
        return {"version": 2, "plugins": {**others, PID: list(records)}}

    def shapes(self):
        """Every list 2.1.272 refuses, 0.62.0 raised on, or charter does not read — a record
        Claude Code cannot read always FIRST, the position the old readers met first."""
        v = self.valid()
        rows = [
            ("not JSON", "{not json"),
            ("nested 200,000 deep", "[" * 200000),
            ("a top-level array", []),
            ("plugins an array", {"version": 2, "plugins": []}),
            ("plugins a string", {"version": 2, "plugins": "x"}),
            ("no version", {"plugins": {PID: [v]}}),
            ("version true", {"version": True, "plugins": {PID: [v]}}),
            ("version 3", {"version": 3, "plugins": {PID: [v]}}),
            ("a version-1 list, which Claude Code migrates and charter does not read",
             {"version": 1, "plugins": {PID: {"version": "0.62.0",
                                              "installedAt": "2026-09-15T13:04:28.952Z",
                                              "installPath": str(self.plugin)}}}),
            ("records a number", {"version": 2, "plugins": {PID: 1}}),
            ("records an object", {"version": 2, "plugins": {PID: v}}),
            ("a string record first", self.listed("junk", v)),
            ("a null record first", self.listed(None, v)),
            ("a number record first", self.listed(7, v)),
            ("a record Claude Code cannot read under another plugin", self.listed(v, **{"other@x": ["junk"]})),
            ("an unknown scope first", self.listed({**v, "scope": "workspace"}, v)),
            ("no scope first", self.listed({k: x for k, x in v.items() if k != "scope"}, v)),
            ("no installPath first", self.listed({k: x for k, x in v.items() if k != "installPath"}, v)),
            ("installPath a number first", self.listed({**v, "installPath": 5}, v)),
            ("projectPath null first", self.listed({**v, "projectPath": None}, v)),
            (f"{OPTIONAL_BOOLEAN} a string first", self.listed({**v, OPTIONAL_BOOLEAN: "yes"}, v)),
            (f"{OPTIONAL_BOOLEAN} a number first", self.listed({**v, OPTIONAL_BOOLEAN: 1}, v)),
            # JSON.parse refuses these three; Python's json reads them (round 4 of the review).
            ("NaN in a field the schema drops", self.listed({**v, "claudeaiPluginId": float("nan")})),
            ("Infinity in a field the schema drops", self.listed({**v, "sourceCommand": float("inf")})),
            ("-Infinity beside the plugins", {"version": 2, "plugins": {PID: [v]}, "x": float("-inf")}),
        ]
        rows += [(f"{field} a number first", self.listed({**v, field: 5}, v))
                 for field in OPTIONAL_STRINGS]
        rows += [(f"the plugin id {key!r} beside charter's", self.listed(v, **{key: [v]}))
                 for key in ("noat", "a@b@c", "-a@b", "a@", "a b@c", ".a@b")]
        return rows


class TestEveryEntryPointSurvivesEveryShape(ListShapeCase):
    def test_the_doctor_rows(self):
        for label, doc in self.shapes():
            with self.subTest(label):
                self.enable()
                self.write(self.manifest, doc)
                # From settings, with no settings file declaring it: the one sighting that makes
                # `guard seen` ask the list who declares the guard now.
                guardseen.mark(harness="claude-code", source=guardseen.SETTINGS)
                wired = doctor.check_guard_wired()
                seen = doctor.check_guard_seen()
                self.assertEqual(wired.status, WARN, f"{wired.detail} {wired.hint}")
                self.assertIn("could not tell", wired.detail)
                self.assertIn(seen.status, (OK, WARN))

    def test_the_session_preflight(self):
        for label, doc in self.shapes():
            with self.subTest(label):
                self.enable()
                self.write(self.manifest, doc)
                out = preflight()
                self.assertIn("could not tell", guard_line(out), out)

    def test_inits_guard_check_wires_the_hook(self):
        """`charter init` and `charter reinit` ask this before writing the plane's settings."""
        for label, doc in self.shapes():
            with self.subTest(label):
                self.enable()
                self.write(self.manifest, doc)
                status, _ = commands._ensure_guard_hook(self.plane)
                self.assertEqual(status, "created")
                self.assertIn("charter hook pretooluse", self.settings.read_text())


class TestEverySettingsShapeEnablesOnlyWhatClaudeCodeReads(ListShapeCase):
    """The settings file the guard row counts its hook block and `enabledPlugins` from.

    A value `JSON.parse` refuses makes the file one Claude Code does not read, so it enables no
    plugin and declares no hook, and `init` leaves it alone as malformed. An `enabledPlugins` that
    is not an object enables no plugin from that file — a list used to raise out of the row and
    out of `init` (round 4 of the review). The install list beside every row is valid and the guard
    has fired from the plugin, so a row that read the file would be green.
    """

    def setUp(self) -> None:
        super().setUp()
        self.write(self.manifest, self.listed(self.valid()))
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)

    def settings_shapes(self):
        return (
            ("enabledPlugins a list", {"enabledPlugins": [PID]}, "created"),
            ("enabledPlugins a string", {"enabledPlugins": PID}, "created"),
            ("enabledPlugins a number", {"enabledPlugins": 1}, "created"),
            ("NaN beside enabledPlugins", {"enabledPlugins": {PID: True}, "x": float("nan")},
             "malformed"),
            ("-Infinity inside enabledPlugins",
             {"enabledPlugins": {PID: True, "other@x": float("-inf")}}, "malformed"),
            ("Infinity beside a hook block that declares the guard",
             {**_PRETOOLUSE, "x": float("inf")}, "malformed"),
        )

    def test_the_guard_row(self):
        for label, doc, status in self.settings_shapes():
            with self.subTest(label):
                self.write(self.settings, doc)
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
                self.assertIn(self.claim_for(status), r.detail)

    def test_the_session_preflight(self):
        for label, doc, status in self.settings_shapes():
            with self.subTest(label):
                self.write(self.settings, doc)
                out = preflight()
                self.assertIn(self.claim_for(status), guard_line(out), out)

    def claim_for(self, init_status: str) -> str:
        """What the row may claim about a settings file, given what `init` did to the same file.

        `malformed` is `init` refusing a file it could not read, and the row may not then say the
        guard is not wired: it did not read the file that would say. It says it could not tell
        (round 7). Where `init` wrote the hook, the file WAS read and "not wired" is a fact.
        """
        return "could not tell" if init_status == "malformed" else _UNWIRED

    def test_inits_guard_check(self):
        for label, doc, status in self.settings_shapes():
            with self.subTest(label):
                self.write(self.settings, doc)
                self.assertEqual(commands._ensure_guard_hook(self.plane)[0], status)


class TestAnInstallPathTheOSCannotCheckIsFilesGone(ListShapeCase):
    """`Path.is_dir()` on a component longer than 255 bytes raises `OSError` 63 (name too long) on
    Python 3.11–3.13 — measured on 3.12 — and answers False on 3.14. Claude Code accepts such a path
    in the list, and `_checks()` has no per-check guard, so the probe took `charter doctor` and the
    preflight down. An `OSError` from it means the files cannot be confirmed: files gone, the row
    warns, `init` writes the hook (round 4 of the review)."""

    LONG = "x" * 300

    def test_a_300_byte_component(self):
        """Red on 3.11–3.13 without the fix. On 3.14 the probe answers without raising, which is
        what the next test is for."""
        self.enable()
        self.write(self.manifest, self.listed({**self.valid(), "installPath": str(self.tmp / self.LONG)}))
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("files are gone", r.detail)
        out = preflight()
        self.assertIn("files are gone", guard_line(out), out)
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "created")

    def test_the_probe_raising_as_3_11_to_3_13_do(self):
        """Red on every version: `Path.is_dir` raises for the install path, as 3.11–3.13 do."""
        self.enable()
        self.write(self.manifest, self.listed(self.valid()))
        real, target = Path.is_dir, str(self.plugin)

        def is_dir(path, *args, **kwargs):
            if str(path) == target:
                raise OSError(63, "File name too long", target)
            return real(path, *args, **kwargs)

        with mock.patch.object(Path, "is_dir", autospec=True, side_effect=is_dir):
            r = doctor.check_guard_wired()
            out = preflight()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("files are gone", r.detail)
        self.assertIn("files are gone", guard_line(out), out)


class TestAListThatMatchesTheSchemaReachesTheSession(ListShapeCase):
    """The controls: without them every test above passes on a reader that refuses everything."""

    def assert_reaches(self, doc) -> None:
        self.enable()
        self.write(self.manifest, doc)
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, f"{r.detail} {r.hint}")
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "present")

    def test_a_record_with_every_typed_field_right(self):
        self.assert_reaches(self.listed(self.full()))

    def test_fields_the_schema_drops_rather_than_refuses(self):
        """Measured: each of these, wrongly typed, and a key the schema does not name, loads."""
        self.assert_reaches(self.listed({
            **self.full(), "claudeaiPluginId": 5, "archiveSha256": 5, "sourceCommand": 5,
            "sourceProducerPath": 5, "previousProducerPaths": 5, "somethingNew": 1}))

    def test_a_plugin_id_the_schema_accepts_beside_charters(self):
        self.assert_reaches(self.listed(self.full(), **{"a-b_c.d@m-1.x": [self.valid()]}))

    def test_the_docs_name_the_schema_they_follow(self):
        self.assertIn(SCHEMA_SOURCE, (REPO / "docs" / "install.md").read_text())


class TestAListClaudeCodeReadsIsReadTheSameWay(ListShapeCase):
    def test_an_enabled_plugin_installed_nowhere_is_unwired_and_not_could_not_tell(self):
        """No list, or a list with no install of it: nothing is installed, which is knowable."""
        for label, doc in (("no list", None), ("no install of it", {"version": 2, "plugins": {}})):
            with self.subTest(label):
                self.enable()
                if doc is None:
                    self.manifest.unlink(missing_ok=True)
                else:
                    self.write(self.manifest, doc)
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
                self.assertIn(_UNWIRED, r.detail)
                self.assertNotIn("could not tell", r.detail)

    def test_another_plugin_that_dispatches_the_guard_is_placed_the_same_way(self):
        """The directory question is asked of whichever enabled plugin declares the guard."""
        old = self.plane.parent / f"{self.plane.name}-before-it-moved"
        self.enable("other@x")
        self.write(self.manifest, {"version": 2, "plugins": {"other@x": [self.valid(old)]}})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("other@x", r.detail)
        self.assertIn(old.name, r.detail)

    def test_an_install_that_dispatches_the_guard_counts_only_if_its_plugin_is_enabled(self):
        """Installed is not enabled (#177), and it stays so when some OTHER plugin is enabled —
        the one case where the list is read at all while charter's own plugin is not enabled."""
        self.enable("other@x")
        self.write(self.manifest, {"version": 2, "plugins": {PID: [self.valid()]}})
        self.assertIsNone(doctor._plugin_declaring_guard())
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn(_UNWIRED, r.detail)
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "created")

    def test_an_installPath_with_a_NUL_byte_is_files_gone_and_not_a_crash(self):
        """A string Claude Code accepts, naming no directory anything can read."""
        self.enable()
        self.write(self.manifest, {"version": 2, "plugins": {PID: [
            {**self.valid(), "installPath": "/x\x00y"}]}})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("files are gone", r.detail)
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "created")


class TestASettingsFileNestedTooDeepIsUnreadable(ListShapeCase):
    """The same `RecursionError` from the other file the guard lookup parses. An unreadable
    settings file enables nothing, and `_ensure_guard_hook` leaves it alone as malformed."""

    def test_the_guard_row_and_inits_guard_check_survive_it(self):
        self.write(self.manifest, {"version": 2, "plugins": {PID: [self.valid()]}})
        self.write(self.settings, "[" * 200000)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        # Not "pretooluse is not wired": charter could not read this file either, so the row says
        # so and names it, exactly as `init` refuses it below (round 7).
        self.assertIn("could not tell", r.detail)
        self.assertIn("settings.json", r.detail)
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "malformed")


class TestASettingsFileTooDeepToRewriteIsLeftAlone(PersonaIso):
    """3.12's `json.dumps` raises `RecursionError` re-dumping a settings file 6,000 levels deep that
    `json.loads` parsed a moment before — measured — and 3.14's does not. So `json.dumps` is made to
    raise that way here, for the one dump that rewrites settings with the guard hook in them, and
    the test is red on every version (round 4 of the review)."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.tmp / "home")}))
        for var in ("CLAUDE_CONFIG_DIR", "CLAUDE_PLUGIN_ROOT", "CLAUDE_CODE_PLUGIN_CACHE_DIR"):
            os.environ.pop(var, None)
        self.settings = config.ROOT / ".claude" / "settings.json"
        self.settings.parent.mkdir(parents=True, exist_ok=True)
        self.settings.write_text(json.dumps(
            {"statusLine": {"type": "command", "command": "echo"}}, indent=2) + "\n")

    @contextmanager
    def too_deep_to_rewrite(self, marker: str = "charter hook pretooluse"):
        """`json.dumps` raises for the rewrite of settings that carry *marker*, and only that one."""
        real = json.dumps

        def dumps(obj, *args, **kwargs):
            if "separators" in kwargs and marker in real(obj):
                raise RecursionError("maximum recursion depth exceeded while encoding a JSON object")
            return real(obj, *args, **kwargs)

        with mock.patch.object(json, "dumps", dumps):
            yield

    def test_the_harness_env_writer_leaves_it_untouched(self):
        """`init` writes `env` into the same file before the guard hook, through the same kind of
        re-dump, so a file too deep to rewrite has to be refused there first."""
        before = self.settings.read_bytes()
        with self.too_deep_to_rewrite("CHARTER_TEST_KEY"):
            status, _ = commands.ensure_env_var(Path(config.ROOT), "CHARTER_TEST_KEY", "x")
        self.assertEqual(status, "malformed")
        self.assertEqual(self.settings.read_bytes(), before)

    def test_the_harness_env_writer_refuses_a_file_nested_too_deeply_to_parse(self):
        """The reader that writer loads through raises `RecursionError` on a file too deep to
        parse, before any rewrite, and `init` runs that writer first."""
        self.settings.write_text("[" * 200000)
        status, _ = commands.ensure_env_var(Path(config.ROOT), "CHARTER_TEST_KEY", "x")
        self.assertEqual(status, "malformed")

    def test_the_guard_check_leaves_it_untouched(self):
        before = self.settings.read_bytes()
        with self.too_deep_to_rewrite():
            status, detail = commands._ensure_guard_hook(Path(config.ROOT))
        self.assertEqual(status, "malformed")
        self.assertEqual(self.settings.read_bytes(), before)

    def test_init_says_so_in_one_contained_sentence_and_exits_non_zero(self):
        err = io.StringIO()
        args = SimpleNamespace(forge="github", owner="acme", host=None, clone_this_repo=False)
        with self.too_deep_to_rewrite(), redirect_stderr(err):
            rc = commands.cmd_init(args)
        self.assertEqual(rc, 1, err.getvalue())
        self.assertTrue(any("settings.json" in line and "left it completely untouched" in line
                            for line in err.getvalue().splitlines()), err.getvalue())

    def test_the_sentence_contains_the_path_it_names(self):
        path = Path("/planes/cfg\n\x1b[2Jboom/.claude/settings.json")
        said = commands._settings_left_untouched(path)
        self.assertNotIn("\n", said)
        self.assertNotIn("\x1b", said)
        self.assertIn("[2Jboom", said)
        self.assertIn("left it completely untouched", said)


class TestTheRowAndTheWriterAgreeAboutOneFile(ListShapeCase):
    """`doctor` and `charter init` may not disagree about whether the guard is wired here.

    Round 6 made the WRITER decide structurally and left the row deciding by substring over the
    same file's raw text, so on three shapes the row printed a green "wired (settings.json)" over
    a plane where nothing runs the guard — and `init`, reading the same file, went on to write
    the hook. A green tick over an unguarded plane is the exact failure this row exists to
    prevent (#168), arrived at from the reader's side.
    """

    #: Shapes Claude Code runs nothing from, each holding the guard's name somewhere the old
    #: substring found it. Hand-spelled, never imported from the module under test.
    NOT_RUN = (
        ("the guard's name in a matcher", {"matcher": "charter hook pretooluse", "hooks": []}),
        ("an entry Claude Code would not run", {"matcher": "Bash", "hooks": [
            {"type": "disabled", "command": "charter hook pretooluse"}]}),
        ("a different handler", {"matcher": "Bash", "hooks": [
            {"type": "command", "command": "charter hook pretooluse-read"}]}),
    )

    def test_a_declaration_claude_code_would_not_run_is_not_a_green_wired(self):
        for label, group in self.NOT_RUN:
            with self.subTest(label):
                self.write(self.settings, {"hooks": {"PreToolUse": [group]}})
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
                self.assertNotIn("wired (", r.detail)
                self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "created")

    def test_a_settings_file_that_is_not_an_object_declares_nothing_and_does_not_crash(self):
        """`JSON.parse` reads `[]`, `"x"`, `7` and `null` perfectly well, and Claude Code loads no
        hooks from any of them. So charter READ this file — it is not a doubt — and it declares
        nothing. Without the type check the reader reaches `doc.get` on a list or a string and
        raises `AttributeError` straight out of the row and out of `init`, which is the shape
        this whole review keeps returning to: a crash where an answer belongs.
        """
        for shape in ("[]", '"x"', "7", "null", "[1, 2]"):
            with self.subTest(shape=shape):
                self.write(self.settings, shape)
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
                self.assertIn(_UNWIRED, r.detail)
                self.assertNotIn("could not tell", r.detail)
                self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "malformed")

    def test_a_plugin_that_is_not_enabled_is_not_a_file_charter_could_not_read(self):
        """Installed is not enabled (#177). A disabled plugin's `hooks.json` decides nothing here,
        so charter being unable to read it is not something to report — the row would otherwise
        say it could not tell, on the strength of a file no session in this plane loads."""
        other = self.tmp / "plugin-cache" / "other"
        self.write(other / "hooks" / "hooks.json", {"hooks": {"PreToolUse": [
            {"matcher": "Bash", "hooks": [
                {"type": "command", "command": "charter hook posttooluse"}]}]}})
        self.write(self.plugin / "hooks" / "hooks.json", "not json")
        self.write(self.settings, {"enabledPlugins": {"other@y": True}})
        self.write(self.manifest, {"version": 2, "plugins": {
            PID: [self.valid()],
            "other@y": [{"scope": "user", "installPath": str(other)}]}})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn(_UNWIRED, r.detail)
        self.assertNotIn("could not tell", r.detail)

    def test_a_real_entry_is_wired_and_the_writer_leaves_it_alone(self):
        self.write(self.settings, _PRETOOLUSE)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, f"{r.detail} {r.hint}")
        self.assertIn("wired", r.detail)
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "present")

    def test_a_settings_file_charter_cannot_parse_is_could_not_tell_not_wired(self):
        for shape in ('{"hooks": {"PreToolUse": NaN}}', "[" * 200000, "not json"):
            with self.subTest(shape=shape[:24]):
                self.write(self.settings, shape)
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
                self.assertIn("could not tell", r.detail)
                self.assertIn("settings.json", r.detail)
                self.assertNotIn("wired (", r.detail)
                self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "malformed")

    def test_whatever_the_row_calls_wired_is_what_the_writer_calls_present(self):
        """The property, asked of every shape at once, so this class cannot come back: one
        reader answers "does the guard run here", and the row and the writer both use it."""
        shapes = [(label, {"hooks": {"PreToolUse": [group]}}) for label, group in self.NOT_RUN]
        shapes += [("a real entry", _PRETOOLUSE),
                   ("no hooks at all", {"enabledPlugins": {}}),
                   ("hooks that are not an object", {"hooks": "PreToolUse"}),
                   ("a group that is not an object", {"hooks": {"PreToolUse": ["x"]}})]
        for label, doc in shapes:
            with self.subTest(label):
                self.write(self.settings, doc)
                says_wired = doctor.check_guard_wired().detail.startswith("wired")
                self.assertEqual(says_wired,
                                 commands._ensure_guard_hook(self.plane)[0] == "present")


class TestAPluginHooksFileCharterCannotReadIsCouldNotTell(ListShapeCase):
    """A plugin's own `hooks.json` that charter cannot parse is not a plugin that does not
    dispatch the guard — it is a file charter could not read, and the row must not say the first
    when it means the second (round 5 of the review).

    `init`'s behaviour is unchanged and deliberately so: no dispatch charter can see means it
    writes the hook, which is the safe direction. What changes is only what the row CLAIMS.
    """

    def setUp(self) -> None:
        super().setUp()
        self.enable()
        self.write(self.manifest, {"version": 2, "plugins": {PID: [self.valid()]}})

    def test_the_row_names_the_file_it_could_not_read_and_init_still_writes_the_hook(self):
        for shape in ('{"hooks": {"PreToolUse": NaN}}', "not json", "[" * 200000):
            with self.subTest(shape=shape[:30]):
                # Back to a plane that declares nothing: the `_ensure_guard_hook` below WROTE a
                # hook block on the previous pass, and a plane that declares the guard itself is
                # a different row (green, and pinned by the case under this one).
                self.enable()
                self.write(self.plugin / "hooks" / "hooks.json", shape)
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
                self.assertIn("could not tell", r.detail)
                self.assertIn("hooks.json", r.detail)
                self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "created")

    def test_a_settings_block_keeps_it_green_and_still_says_what_it_could_not_read(self):
        """The block a session here loads runs whatever the plugin's file holds, so the row is
        green — and still names the file, because "could not tell whether a plugin ALSO
        dispatches it" is then the only thing left unsaid. The same sentence round 3 gave an
        install list charter cannot read, for a file one level down."""
        self.write(self.settings, {"enabledPlugins": {PID: True}, **_PRETOOLUSE})
        self.write(self.plugin / "hooks" / "hooks.json", '{"hooks": {"PreToolUse": NaN}}')
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, f"{r.detail} {r.hint}")
        self.assertIn("could not tell", r.detail)
        self.assertIn("hooks.json", r.detail)

    def test_a_plugin_that_does_dispatch_it_settles_the_row_whatever_else_cannot_be_read(self):
        """The doubt is asked only when nothing was found. A plugin charter CAN read, dispatching
        the guard, answers the row — and another plugin's unreadable file must not put a caveat
        on an answer that does not depend on it."""
        other = self.tmp / "plugin-cache" / "other"
        self.write(other / "hooks" / "hooks.json", "not json")
        self.write(self.settings, {"enabledPlugins": {PID: True, "other@x": True}})
        self.write(self.manifest, {"version": 2, "plugins": {
            PID: [self.valid()],
            "other@x": [{"scope": "user", "installPath": str(other)}]}})
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, f"{r.detail} {r.hint}")
        self.assertNotIn("could not tell", r.detail)

    def test_it_names_two_of_the_files_it_could_not_read_and_not_every_one(self):
        """A row is one line, and the install list is a file a chat can write — so the count of
        files it can put in this sentence is not the count of files it may print."""
        ids, names = {}, ("plugA", "plugB", "plugC")
        for name in names:
            at = self.tmp / "plugin-cache" / name
            self.write(at / "hooks" / "hooks.json", "not json")
            ids[f"{name}@x"] = [{"scope": "user", "installPath": str(at)}]
        self.write(self.settings, {"enabledPlugins": {pid: True for pid in ids}})
        self.write(self.manifest, {"version": 2, "plugins": ids})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("plugA", r.detail)
        self.assertIn("plugB", r.detail)
        self.assertNotIn("plugC", r.detail)

    def test_a_hooks_file_that_simply_does_not_dispatch_is_not_doubt(self):
        """The row may only say it could not read a file when it could not. A plugin wiring some
        other handler is read perfectly well and dispatches nothing."""
        self.write(self.plugin / "hooks" / "hooks.json", {"hooks": {"PreToolUse": [
            {"matcher": "Bash", "hooks": [
                {"type": "command", "command": "charter hook pretooluse-read"}]}]}})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertNotIn("could not tell", r.detail)
        self.assertIn(_UNWIRED, r.detail)


class TestAListClaudeCodeReadsFromElsewhereIsCouldNotTell(ListShapeCase):
    """`$CLAUDE_CODE_PLUGIN_CACHE_DIR` moves the install list out of the config folder. Measured:
    set to an empty directory, `claude plugin list --json` lists no install; set empty, it lists
    them all. Charter does not follow it, so the list it would read is not the one Claude Code
    reads."""

    def setUp(self) -> None:
        super().setUp()
        self.enable()
        self.write(self.manifest, {"version": 2, "plugins": {PID: [self.valid()]}})
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)

    def test_set_it_is_could_not_tell_and_init_wires_the_hook(self):
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_PLUGIN_CACHE_DIR": str(self.tmp / "moved")}):
            r = doctor.check_guard_wired()
            status, _ = commands._ensure_guard_hook(self.plane)
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("could not tell", r.detail)
        self.assertIn("CLAUDE_CODE_PLUGIN_CACHE_DIR", r.detail)
        self.assertEqual(status, "created")

    def test_set_empty_claude_code_does_not_follow_it_either(self):
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_PLUGIN_CACHE_DIR": ""}):
            r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, f"{r.detail} {r.hint}")


if __name__ == "__main__":
    unittest.main()
