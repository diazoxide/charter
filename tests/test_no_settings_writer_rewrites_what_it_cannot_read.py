"""Charter never writes back a settings file it did not read the way the harness reads it.

Round 5 of the review of `7abb4ee`, which is three findings of one shape: a *reader* and a
*writer* of the same file disagreeing about whether charter can read it.

**A file too deep to re-encode (D1).** On Python 3.12 a 6,000-level settings file parses and
then `json.dumps` raises `RecursionError` re-encoding it — measured locally on 3.12.13; 3.11
refuses to parse the same text, and 3.14 does neither. Round 4 caught that in
`commands._ensure_guard_hook` and `commands.ensure_env_var` alone, with a patch keyed to the
text those two writers add. `commands.add_permission_rule` adds different text, so the patch
never reached it and `charter init` still died with an uncaught `RecursionError` through
`ensure_handoff_gate` → `add_ask_rule` → `add_permission_rule`.

So the enumeration is the test here, not one writer: `commands._guard_apply` commits through
``getattr(h, method)`` over `harness.registry.all()`, so every registered harness is a writer
of its own file, and every one of them is driven below. The patch is on `json.dumps` — the one
level every writer re-encodes through — and it fires on the *depth* of the document rather than
on any writer's own text, which is what makes it reach all of them at once.

**A file the harness itself refuses (D2).** `JSON.parse` refuses `NaN`, `Infinity` and
`-Infinity`; Python's `json` accepts them. `doctor._json_as_claude_code_parses` is the reader
that says so, and `commands._load_settings` and `commands._load_json_settings` still called
plain `json.loads` — so with a `NaN` in `.claude/settings.json`, `init` rewrote the file
through those two loaders, adding `env.CHARTER_HARNESS` and a `permissions.ask` entry, and then
printed that it had "left it completely untouched". Both halves were wrong: it wrote into a file
Claude Code loads nothing from, and it said it had not.

**A hook entry the harness would never run (D3).** `doctor._dispatches_guard` asked whether the
text `charter hook pretooluse` appeared anywhere in a plugin's `hooks.json`. That counts the
string in a `matcher`, in a description, under another event, in an entry Claude Code does not
run as a command — and it counts `charter hook pretooluse-read`, a different handler that
guards nothing on Bash. It is decided structurally here instead, from the entry Claude Code
would actually run, the way `wiring._guard_positions` already reads the same file for Codex.
"""

from __future__ import annotations

import io
import json
import os
import unittest
from contextlib import contextmanager, redirect_stderr
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, doctor
from charter.harness import opencode as _opencode
from tests._isolation import PersonaIso

#: Levels at which 3.12 parses a settings file and then cannot re-encode it. Measured locally:
#: 3.11 raises `RecursionError` parsing this text, 3.12.13 parses it and `json.dumps` raises,
#: 3.14.4 does neither. The depth is therefore never asserted on — `_parses_but_cannot_be_rewritten`
#: asks this interpreter what it does, and the portable half patches instead.
GENUINELY_TOO_DEEP = 6000

#: What `init` says about a settings file it would not rewrite. Hand-spelled, never imported.
_UNTOUCHED = "left it completely untouched"


def deep_text(levels: int) -> str:
    """A settings file nested *levels* deep, as text.

    Built as text rather than by `json.dumps` of a built object, because on the interpreter
    this file is about, encoding it is the thing that raises.

    **Indented, and that is load-bearing.** Measured on 3.12.13 against a 6,000-deep document:
    `json.dumps` raises `RecursionError` for `indent=2` and for the `indent="  "` charter
    passes, and returns happily for `indent=None` — the C encoder, which the module uses when
    there is no indent. `commands._json_style` reads the indent out of the file being rewritten,
    so a real `.claude/settings.json` (indented) is re-encoded by the Python encoder and a
    minified one is not. A fixture written on one line would therefore be re-encodable even on
    3.12 and would prove nothing about the writers.
    """
    return ('{\n  "statusLine": {"type": "command", "command": "echo"},\n  "deep": '
            + '{"deep": ' * levels + "{}" + "}" * levels + "\n}\n")


#: **Measured, and deliberately not run here.** On 3.14 a settings file whose hook group nests
#: 70,000 levels parses and the C encoder then refuses it (60,000 still encodes); 3.12's decoder
#: gives up by 20,000 and never reaches the encoder at all. Dict nesting, because that is what a
#: hook group is.
#:
#: That case is documented rather than asserted, and the reason is worth the paragraph. Driven at
#: 100,000 it cost the `test (3.14)` job on `a31177c` — killed 22 seconds into the one test. At
#: 70,000 it was worse than slow: building that object graph *at import time*, for a `skipUnless`
#: probe, made this module poison every other module sharing its process. Measured on 3.14, this
#: module plus `test_every_reader_of_the_install_list_survives_its_shape` went from 17 seconds to
#: 192, almost all of it system time, and stayed slow even with the deep TEST deselected — while
#: the same pair on 3.12, where the probe fails cheaply at the parse, stayed at 13. A fixture
#: that costs a CI job is a defect in the fixture; one that slows every test after it is worse,
#: because nothing points at the module that did it.
#:
#: So the encoder's refusal is proved the portable way instead — `too_deep_to_rewrite` patches
#: `json.dumps` on the document's DEPTH, which asks the same question of the same code at a depth
#: every machine survives, and is red on every version rather than on 3.14 alone.


def deep_group_text(levels: int) -> str:
    """A settings file whose `hooks.PreToolUse` holds one group nested *levels* deep.

    The nesting is INSIDE the group on purpose. `_ensure_guard_hook` used to re-encode each
    group of `pre` one at a time to search it, so only a deep GROUP drives that line — round 5's
    fixture nested under a top-level key, left `pre` empty, and never reached it.
    """
    nest = '{"deep": ' * levels + "{}" + "}" * levels
    return ('{\n  "statusLine": {"type": "command", "command": "echo"},\n'
            '  "hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [], "deep": '
            + nest + "}]}\n}\n")


def depth(obj) -> int:
    """How deeply *obj* nests, walked with an explicit stack.

    Iterative on purpose: a recursive measurement would hit the interpreter's own limit on the
    documents these tests are about, which is the failure under test rather than a way to
    measure it.
    """
    deepest, stack = 0, [(obj, 1)]
    while stack:
        node, at = stack.pop()
        deepest = max(deepest, at)
        if isinstance(node, dict):
            stack.extend((v, at + 1) for v in node.values())
        elif isinstance(node, list):
            stack.extend((v, at + 1) for v in node)
    return deepest


def interpreter_refuses(text: str) -> str | None:
    """Where THIS interpreter gives up on *text*: ``"parse"``, ``"encode"``, or ``None``.

    Measured locally: 3.11 answers ``"parse"`` for a 6,000-deep file, 3.12.13 answers
    ``"encode"`` — it parses the file and `json.dumps` raises re-encoding it — and 3.14.4
    answers ``None``, because it does both without complaint.

    The encode is probed WITH an indent, because that is how charter re-encodes an indented
    settings file (`commands._json_style`) and the indentless C encoder does not raise at all —
    see :func:`deep_text`.

    A test that asserted "`init` refuses this file" on every version would be asserting a
    defect where there is none: on 3.14 the file is perfectly writable, and charter writing
    it is the correct answer. So the cases below ask this first.
    """
    try:
        doc = json.loads(text)
    except RecursionError:
        return "parse"
    try:
        json.dumps(doc, indent=2)
    except RecursionError:
        return "encode"
    return None


@contextmanager
def too_deep_to_rewrite(limit: int = 50):
    """`json.dumps` raises `RecursionError` for any document nested deeper than *limit*.

    3.12's encoder does exactly this at 6,000 levels and 3.14's does not, so the depth a real
    file needs is not portable and this stands in for it. Keyed on the document's DEPTH, which
    is what makes it a patch every writer passes through: round 4's version fired only for the
    text `_ensure_guard_hook` and `ensure_env_var` add, so `add_permission_rule` — which adds
    its own — re-encoded happily underneath it and was still uncaught two rounds later.
    """
    real = json.dumps

    def dumps(obj, *args, **kwargs):
        if depth(obj) > limit:
            raise RecursionError("maximum recursion depth exceeded while encoding a JSON object")
        return real(obj, *args, **kwargs)

    with mock.patch.object(json, "dumps", dumps):
        yield


class SettingsCase(PersonaIso):
    """A plane whose `.claude/settings.json` holds something charter must not rewrite."""

    def setUp(self) -> None:
        super().setUp()
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.tmp / "home")}))
        for var in ("CLAUDE_CONFIG_DIR", "CLAUDE_PLUGIN_ROOT", "CLAUDE_CODE_PLUGIN_CACHE_DIR"):
            os.environ.pop(var, None)
        self.root = Path(config.ROOT)
        self.settings = self.root / ".claude" / "settings.json"
        self.local = self.root / commands.LOCAL_SETTINGS
        self.opencode = self.root / "opencode.json"

    def put(self, path: Path, text: str) -> bytes:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path.read_bytes()

    def init(self) -> tuple[int, str]:
        """`charter init` end to end, with its stderr."""
        err = io.StringIO()
        args = SimpleNamespace(forge="github", owner="acme", host=None, clone_this_repo=False)
        with redirect_stderr(err):
            rc = commands.cmd_init(args)
        return rc, err.getvalue()


class TestEveryWriterRefusesAFileItCannotReEncode(SettingsCase):
    """D1. Every path that rewrites a settings file answers `malformed` and changes nothing.

    The deep document here parses on every supported version; `too_deep_to_rewrite` supplies
    what only 3.12 supplies for a real one.
    """

    def setUp(self) -> None:
        super().setUp()
        self.before = self.put(self.settings, deep_text(200))
        self.opencode_before = self.put(self.opencode, deep_text(200))

    def unchanged(self) -> None:
        self.assertEqual(self.settings.read_bytes(), self.before)
        self.assertEqual(self.opencode.read_bytes(), self.opencode_before)

    def test_the_ask_rule_writer_refuses_it(self):
        """`add_permission_rule`, which `init` reaches through `ensure_handoff_gate` and which
        round 4's patch never touched."""
        with too_deep_to_rewrite():
            status, _detail = commands.add_ask_rule(self.root, "Bash(terraform apply *)")
        self.assertEqual(status, "malformed")
        self.unchanged()

    def test_the_allow_rule_writer_refuses_it(self):
        with too_deep_to_rewrite():
            status, _detail = commands.add_allow_rule(self.root, "Bash(git status *)")
        self.assertEqual(status, "malformed")
        self.unchanged()

    def test_the_machine_local_rule_writer_refuses_it(self):
        """`--local` writes the other file through the same function and the other loader."""
        before = self.put(self.local, deep_text(200))
        with too_deep_to_rewrite():
            status, _detail = commands.add_ask_rule(self.root, "Bash(x *)", local=True)
        self.assertEqual(status, "malformed")
        self.assertEqual(self.local.read_bytes(), before)

    def test_the_env_writer_refuses_it(self):
        with too_deep_to_rewrite():
            status, _detail = commands.ensure_env_var(self.root, "CHARTER_TEST_KEY", "x")
        self.assertEqual(status, "malformed")
        self.unchanged()

    def test_the_guard_hook_writer_refuses_it(self):
        with too_deep_to_rewrite():
            status, _detail = commands._ensure_guard_hook(self.root)
        self.assertEqual(status, "malformed")
        self.unchanged()

    def test_opencodes_rule_writer_refuses_it(self):
        """opencode writes `opencode.json` itself — a second harness, a second file, the same
        re-encode. `_guard_apply` commits through it exactly as it does Claude Code's."""
        with too_deep_to_rewrite():
            status, _detail = _opencode.OpenCodeHarness().apply_ask_rule(
                self.root, "terraform apply *")
        self.assertEqual(status, "malformed")
        self.unchanged()

    def test_opencodes_instructions_writer_refuses_it(self):
        """`OpenCodeHarness.wire`'s own writer, which `init` runs through `_wire_harnesses`."""
        with too_deep_to_rewrite():
            status = _opencode.ensure_instructions(self.root)
        self.assertEqual(status, "malformed")
        self.unchanged()

    def test_every_registered_harness_refuses_it_and_none_raises(self):
        """The enumeration itself: `_guard_apply` asks every harness in the registry, so a
        harness that writes its own file is covered the day it is registered."""
        from charter.harness import registry

        for h in registry.all():
            for method in ("apply_ask_rule", "apply_allow_rule"):
                with self.subTest(harness=h.name, method=method):
                    with too_deep_to_rewrite():
                        status, _detail = getattr(h, method)(self.root, "terraform apply *")
                    self.assertIn(status, ("malformed", "unsupported"))
                    self.unchanged()

    def test_the_transaction_writes_nowhere_and_says_so(self):
        """`_guard_apply`'s own answer: blocked, so no harness is committed."""
        with too_deep_to_rewrite():
            results, blocked = commands._guard_apply(
                "apply_ask_rule", self.root, "terraform apply *", False)
        self.assertTrue(blocked, results)
        self.unchanged()

    def test_every_harness_answers_a_dry_run_the_way_it_would_answer_writing(self):
        """`dry_run` is the write path minus the write, and `_guard_apply` decides the WHOLE
        transaction on it (#376).

        Asked of each harness on its own, because the transaction's own answer does not pin
        this: `_guard_apply` blocks when ANY harness refuses, so one harness answering
        `malformed` in its check hides another whose check says `added` and whose commit then
        says `malformed`. That is the split the two-phase commit exists to prevent — the
        transaction decides it is safe, commits, and meets the refusal with the earlier harness
        already written — and it is what the first cut of this fix did, in both harnesses.
        """
        from charter.harness import registry

        for h in registry.all():
            with self.subTest(harness=h.name):
                with too_deep_to_rewrite():
                    checked, _detail = h.apply_ask_rule(self.root, "terraform apply *",
                                                        dry_run=True)
                    committed, _d = h.apply_ask_rule(self.root, "terraform apply *")
                self.assertEqual(checked, committed)
                self.unchanged()

    def test_the_writers_refuse_a_file_too_deep_to_PARSE_as_well(self):
        """The other end of the same file: a document nested past the decoder's limit raises
        `RecursionError` out of the READER, which is not a `ValueError` and so is not covered by
        a decoder's own catch. No patch here — 200,000 levels is beyond every supported
        interpreter's parser, so this one is real on all four.
        """
        settings_before = self.put(self.settings, "[" * 200000)
        opencode_before = self.put(self.opencode, "[" * 200000)
        self.assertEqual(commands.add_ask_rule(self.root, "Bash(x *)")[0], "malformed")
        self.assertEqual(
            _opencode.OpenCodeHarness().apply_ask_rule(self.root, "terraform apply *")[0],
            "malformed")
        self.assertEqual(_opencode.ensure_instructions(self.root), "malformed")
        self.assertEqual(self.settings.read_bytes(), settings_before)
        self.assertEqual(self.opencode.read_bytes(), opencode_before)

    def test_init_refuses_and_leaves_it_byte_identical(self):
        with too_deep_to_rewrite():
            rc, err = self.init()
        self.assertEqual(rc, 1, err)
        self.unchanged()
        self.assertIn(_UNTOUCHED, err)


@unittest.skipIf(interpreter_refuses(deep_text(GENUINELY_TOO_DEEP)) is None,
                 "this interpreter parses and re-encodes a 6,000-deep settings file, so there "
                 "is nothing here for it to refuse")
class TestAGenuinelyTooDeepFileNeverCrashesInit(SettingsCase):
    """D1, driven end to end with a real file rather than a patch.

    Where the interpreter gives up differs — 3.11 will not parse the file, 3.12 parses it and
    cannot re-encode it — and `init`'s answer must not: no traceback, exit 1, and the file
    exactly as it was. On 3.14, which does both, the whole class is skipped rather than
    asserting a refusal that would be wrong there.
    """

    def setUp(self) -> None:
        super().setUp()
        self.text = deep_text(GENUINELY_TOO_DEEP)
        self.before = self.put(self.settings, self.text)

    def test_init_exits_one_and_writes_nothing(self):
        rc, err = self.init()
        self.assertEqual(rc, 1, err)
        self.assertEqual(self.settings.read_bytes(), self.before)
        self.assertIn(_UNTOUCHED, err)

    @unittest.skipUnless(interpreter_refuses(deep_text(GENUINELY_TOO_DEEP)) == "encode",
                         "this interpreter refuses the file at the parse, before any writer")
    def test_the_rule_writer_is_the_one_that_dies_here(self):
        """On 3.12 the file parses, so every reader hands it on and the *writers* are what
        `init` has to survive — `add_permission_rule` first, which is where it died."""
        status, _detail = commands.add_ask_rule(self.root, "Bash(terraform apply *)")
        self.assertEqual(status, "malformed")
        self.assertEqual(self.settings.read_bytes(), self.before)


class TestNothingIsWrittenBackThatClaudeCodeRefuses(SettingsCase):
    """D2. `JSON.parse` refuses `NaN`; charter must refuse to write the file back too."""

    def setUp(self) -> None:
        super().setUp()
        self.text = '{"statusLine": {"type": "command", "command": "echo"}, "x": NaN}'
        self.before = self.put(self.settings, self.text)

    def test_the_committed_loader_reads_it_as_unusable(self):
        doc, path = commands._load_settings(self.root)
        self.assertIsNone(doc)
        self.assertEqual(Path(path), self.settings)

    def test_the_machine_local_loader_reads_it_as_unusable(self):
        before = self.put(self.local, self.text)
        doc, _path = commands._load_json_settings(self.local)
        self.assertIsNone(doc)
        self.assertEqual(self.local.read_bytes(), before)

    def test_the_env_writer_refuses_it(self):
        status, _path = commands.ensure_env_var(self.root, "CHARTER_HARNESS", "claude-code")
        self.assertEqual(status, "malformed")
        self.assertEqual(self.settings.read_bytes(), self.before)

    def test_the_ask_rule_writer_refuses_it(self):
        status, _detail = commands.add_ask_rule(self.root, "Bash(charter handoff *)")
        self.assertEqual(status, "malformed")
        self.assertEqual(self.settings.read_bytes(), self.before)

    def test_init_writes_nothing_exits_one_and_its_sentence_is_true(self):
        """The whole finding in one case: on `7abb4ee` this file came back holding
        `env.CHARTER_HARNESS` and `permissions.ask`, under a sentence saying it had not been
        touched."""
        rc, err = self.init()
        self.assertEqual(rc, 1, err)
        self.assertEqual(self.settings.read_bytes(), self.before, "init rewrote the file")
        self.assertNotIn("CHARTER_HARNESS", self.settings.read_text())
        self.assertNotIn("permissions", self.settings.read_text())
        self.assertIn(_UNTOUCHED, err)
        self.assertIn("settings.json", err)

    def test_it_is_never_listed_as_already_present(self):
        """A file charter refused is not a file it found its key in — `init` listed the env
        write under "already present" while refusing the same file two lines down."""
        _rc, err = self.init()
        for line in err.splitlines():
            if "already present" in line:
                self.assertNotIn("settings.json (env)", line, err)

    def test_the_other_two_constants_are_refused_the_same_way(self):
        for constant in ("Infinity", "-Infinity"):
            with self.subTest(constant=constant):
                before = self.put(self.settings, '{"a": %s}' % constant)
                status, _detail = commands.add_ask_rule(self.root, "Bash(x *)")
                self.assertEqual(status, "malformed")
                self.assertEqual(self.settings.read_bytes(), before)


class TestTheWiredCheckDecidesOnTheParsedEntry(SettingsCase):
    """`_ensure_guard_hook` asks whether the guard is ALREADY wired in this settings file, and it
    answered by re-encoding each `hooks.PreToolUse` group and searching the text (round 6).

    Two defects in one line. The search counted the guard's name anywhere in the group, including
    places Claude Code runs nothing from — so `init` wrote no hook for a plane nothing guarded.
    And the re-encode existed *only* to be searched, one line above the re-encode that round 4
    guarded: a group nested past the C encoder's limit raised `RecursionError` there, so `charter
    init` died with no sentence and no exit code. It is decided on the parsed entry now, through
    the reader `doctor` uses for a plugin's own `hooks.json`.
    """

    def group(self, *entries, matcher: str = "Bash") -> bytes:
        return self.put(self.settings, json.dumps(
            {"hooks": {"PreToolUse": [{"matcher": matcher, "hooks": list(entries)}]}}, indent=2))

    def test_the_guards_name_in_a_matcher_is_not_wired(self):
        before = self.group(matcher="charter hook pretooluse")
        status, _detail = commands._ensure_guard_hook(self.root)
        self.assertEqual(status, "created")
        self.assertNotEqual(self.settings.read_bytes(), before)

    def test_the_guards_name_in_an_entry_claude_code_would_not_run_is_not_wired(self):
        self.group({"type": "disabled", "command": "charter hook pretooluse"})
        self.assertEqual(commands._ensure_guard_hook(self.root)[0], "created")

    def test_another_handler_is_not_this_guard(self):
        self.group({"type": "command", "command": "charter hook pretooluse-read"})
        self.assertEqual(commands._ensure_guard_hook(self.root)[0], "created")

    def test_a_real_entry_is_still_wired_and_nothing_is_written(self):
        before = self.group({"type": "command", "command": "charter hook pretooluse",
                             "timeout": 10})
        status, _detail = commands._ensure_guard_hook(self.root)
        self.assertEqual(status, "present")
        self.assertEqual(self.settings.read_bytes(), before)

    def test_a_group_too_deep_to_re_encode_never_raises(self):
        """Patched, so it is red on every version: the depth that breaks a real encoder is not
        portable, and the old line re-encoded each group whatever its depth."""
        before = self.put(self.settings, deep_group_text(200))
        with too_deep_to_rewrite():
            status, _detail = commands._ensure_guard_hook(self.root)
        self.assertEqual(status, "malformed")
        self.assertEqual(self.settings.read_bytes(), before)

    def test_init_refuses_a_group_it_cannot_re_encode(self):
        """`init` end to end over a populated `pre`: one contained sentence, exit 1, and the file
        exactly as it was — never a traceback.

        Patched rather than genuinely deep, and that is a lesson this fixture paid for. Driven at
        100,000 real levels it cost the `test (3.14)` job on `a31177c`, killed 22 seconds into
        this one test; `cmd_init` attempts the encode in several writers, and a descent that deep
        is a native stack problem on the runner rather than a slow one. The patch asks the same
        question of the same code at a depth every machine survives, and the real encoder is
        still measured — once, on one writer, below.
        """
        before = self.put(self.settings, deep_group_text(200))
        with too_deep_to_rewrite():
            rc, err = self.init()
        self.assertEqual(rc, 1, err)
        self.assertEqual(self.settings.read_bytes(), before)
        self.assertIn(_UNTOUCHED, err)

    def test_a_group_charter_cannot_parse_is_refused_too(self):
        """The other end of the same file, and genuinely deep rather than patched — 200,000
        levels is beyond every supported decoder, so it fails at the parse in milliseconds
        instead of building the object graph that made the encoder case unaffordable."""
        before = self.put(self.settings, '{"hooks": {"PreToolUse": [' + "[" * 200000)
        status, _detail = commands._ensure_guard_hook(self.root)
        self.assertEqual(status, "malformed")
        self.assertEqual(self.settings.read_bytes(), before)


class TestCodexRefusesAConfigItCannotParse(SettingsCase):
    """D1's third harness. Codex's writer reads `$CODEX_HOME/config.toml` and appends whole
    tables to it, so that read is what decides the append.

    `tomllib.loads` raises `RecursionError` on a config nested too deeply — measured at 5,000
    levels of inline array on 3.11, 3.12 and 3.14 alike, so unlike the two JSON encoders this
    one needs no stand-in and the file here is genuinely too deep on every version. Uncaught, it
    ended `charter harness install` on a file charter can simply refuse.
    """

    def test_it_refuses_and_appends_nothing(self):
        from charter.harness import codex as _codex

        home = self.tmp / "codex-home"
        config_toml = home / "config.toml"
        before = self.put(config_toml, "a = " + "[" * 5000 + "]" * 5000 + "\n")
        status, _detail = _codex.install(env={"CODEX_HOME": str(home)})
        self.assertEqual(status, "malformed")
        self.assertEqual(config_toml.read_bytes(), before)


class TestOnlyAnEntryClaudeCodeRunsIsADispatch(SettingsCase):
    """D3. A plugin dispatches the guard when an entry Claude Code would RUN says so."""

    def setUp(self) -> None:
        super().setUp()
        self.plugin = self.tmp / "plugin-cache" / "charter"

    def hooks_json(self, doc) -> str:
        p = self.plugin / "hooks" / "hooks.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(doc if isinstance(doc, str) else json.dumps(doc))
        return str(self.plugin)

    def entry(self, command: str, kind: str = "command") -> dict:
        return {"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": kind, "command": command}]}]}}

    def test_a_real_entry_is_a_dispatch(self):
        at = self.hooks_json(self.entry("charter hook pretooluse --plugin-version 0.62.0"))
        self.assertTrue(doctor._dispatches_guard(at))

    def test_the_text_in_a_matcher_is_not(self):
        at = self.hooks_json({"hooks": {"PreToolUse": [
            {"matcher": "charter hook pretooluse", "hooks": []}]}})
        self.assertFalse(doctor._dispatches_guard(at))

    def test_the_text_beside_an_entry_is_not(self):
        """A description, a comment key, anything that is not the command Claude Code runs."""
        at = self.hooks_json({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command", "command": "echo hi",
             "statusMessage": "charter hook pretooluse"}]}]}})
        self.assertFalse(doctor._dispatches_guard(at))

    def test_an_entry_claude_code_would_not_run_is_not(self):
        """Claude Code runs `type: "command"` entries. Anything else declares no command,
        so the guard's text in one dispatches nothing."""
        at = self.hooks_json(self.entry("charter hook pretooluse", kind="disabled"))
        self.assertFalse(doctor._dispatches_guard(at))

    def test_another_handler_is_not_the_guard(self):
        """`pretooluse-read` guards Read and Grep, not Bash. Matched as a substring it read as
        the Bash guard, so a plugin wiring only the read handler counted as wiring this one."""
        at = self.hooks_json(self.entry("charter hook pretooluse-read --plugin-version 0.62.0"))
        self.assertFalse(doctor._dispatches_guard(at))

    def test_another_event_is_not_the_guard(self):
        at = self.hooks_json({"hooks": {"PostToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command", "command": "charter hook pretooluse"}]}]}})
        self.assertFalse(doctor._dispatches_guard(at))

    def test_a_file_claude_code_cannot_read_is_not_a_dispatch(self):
        """And so `init` writes the hook — the safe direction: declared twice is harmless and
        reported, declared nowhere is a hole."""
        for shape in ('{"hooks": {"PreToolUse": NaN}}', "[" * 200000, "not json", "[]",
                      '{"hooks": {"PreToolUse": {"matcher": "Bash"}}}',
                      '{"hooks": {"PreToolUse": ["charter hook pretooluse"]}}',
                      # `hooks` itself not an object — the outermost level, and the one a walk
                      # that trusted the file would reach first.
                      '{"hooks": "PreToolUse"}', '{"hooks": 7}', '{"hooks": null}'):
            with self.subTest(shape=shape[:40]):
                self.assertFalse(doctor._dispatches_guard(self.hooks_json(shape)))

    def test_an_entry_that_is_not_an_object_dispatches_nothing(self):
        """Every level of this file is a line a chat can write, and the ENTRY level is the one
        the shapes above stop short of: a string inside `hooks` is reached only once the group
        around it is an object, so a test whose group is a string never gets here at all."""
        for entries in (["charter hook pretooluse"], [None], [7], "charter hook pretooluse",
                        {"command": "charter hook pretooluse"}):
            with self.subTest(entries=entries):
                at = self.hooks_json({"hooks": {"PreToolUse": [
                    {"matcher": "Bash", "hooks": entries}]}})
                self.assertFalse(doctor._dispatches_guard(at))

    def test_an_entry_whose_command_is_not_a_string_dispatches_nothing(self):
        """The last level a chat can write: an entry that says `type: "command"` and then gives
        something that is not one. The handler match runs a regex over it, which raises on a
        number or a list rather than answering no."""
        for command in (7, None, ["charter hook pretooluse"],
                        {"run": "charter hook pretooluse"}, True):
            with self.subTest(command=command):
                at = self.hooks_json({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
                    {"type": "command", "command": command}]}]}})
                self.assertFalse(doctor._dispatches_guard(at))

    def test_an_entry_with_no_command_at_all_dispatches_nothing(self):
        at = self.hooks_json({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command"}]}]}})
        self.assertFalse(doctor._dispatches_guard(at))

    def test_the_shipped_plugin_still_dispatches_it(self):
        """The reading has to keep answering yes for charter's own `hooks/hooks.json`, whose
        PreToolUse block also carries three handlers that are not this one."""
        repo = Path(__file__).resolve().parents[1]
        self.assertTrue(doctor._dispatches_guard(str(repo)))


if __name__ == "__main__":
    unittest.main()
