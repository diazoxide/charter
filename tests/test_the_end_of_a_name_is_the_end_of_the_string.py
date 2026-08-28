"""`$` is a SPELLING of "the end of the string". It is not the end of the string (#577).

In Python, ``$`` matches at the end of the string **or just before a trailing newline**, and
``re.Pattern.match`` anchors only the front. So ``^[a-z0-9][a-z0-9._-]*$`` asked with
``.match`` admits its whole alphabet *plus* ``"\\n"`` — and every anchored name rule charter
had was written that way. ``persona.valid_name("evil\\n")`` was True, ``personas/evil<LF>/``
resolved and loaded, and ``persona sync-agents`` wrote the name — newline and all — into a
generated agent's YAML frontmatter, splitting the file across a blank line. `mcpseen.label`'s
docstring states the invariant that broke, in as many words.

This is the fifth instance of one root cause on this project (#547, #558, #537, #498, #577):
a check that matches a spelling instead of the property it means. `re.fullmatch` **is** the
property; ``$`` is a spelling of it that is right most of the time.

**Why this file sweeps instead of asserting four cases.** A test that pins
``valid_name("evil\\n") is False`` pins the instance, and the tenth anchored rule somebody
adds next month inherits nothing from it. So the inventory is *derived* — every anchored
pattern in `charter/` is discovered here, and every one of them has to be classified as an
ADMITTER or a DETECTOR before this file will pass. A tenth regex is a failure until someone
says which it is, and an admitter is then swept automatically.

**Admitter and detector are opposite, and the same `$` is a defect in one and load-bearing
in the other.** An *admitter* answers "is this value acceptable?" — over-matching admits a
value the rule exists to refuse, which is #577. A *detector* answers "is this token a
redirection / a duration / a config key I must account for?" — over-matching makes a guard
fire on MORE inputs, and tightening it makes the guard fire on FEWER. Measured, on the real
guards, in `TestTighteningADetectorWouldFailOpen`: switching `hooks._REDIRECT_READ_RE` to
`fullmatch` stops `_redirect_reads` seeing a vault the shell opens, and switching
`hooks._DURATION_RE` makes `_split_env_chdir` name ``'5\\n'`` as the program instead of
``cat``. Those four in `charter/hooks.py` and one in `charter/toolgate.py` therefore keep
``.match`` **on purpose**, and this file pins that so a later sweep does not "finish the job"
into a fail-open guard.

**What is NOT special.** ``\\r``, U+2028 LINE SEPARATOR and U+0085 NEXT LINE are frequently
assumed to be line terminators for ``$``. In Python's `re` they are not — only U+000A is.
`TestWhatPythonsDollarActuallyIs` measures that over the whole codespace rather than
repeating the assumption, so the fix is scoped to what the engine actually does and a future
Python that widened it would fail here first.
"""

from __future__ import annotations

import ast
import importlib
import io
import pkgutil
import re
import string
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

import charter
from charter import (browser, commands, commands_persona, commands_secrets, config,
                     docsrc, hooks, instance, persona, plugincache, recall, toolgate)
from charter.forge import registry
from charter.frame import component
from tests._isolation import PersonaIso

#: Written as an escape rather than as a literal newline, for the reason #498 learned about
#: U+3164: the character this file is about is invisible in the editor of whoever reads it
#: next, and that invisibility is exactly how it travelled.
LF = "\n"

#: The three terminators people *believe* ``$`` honours, plus the two ASCII vertical spaces.
#: `TestWhatPythonsDollarActuallyIs` measures that none of them is special, which is why the
#: fix needs no normalisation pass — one codepoint, one word per call site.
ASSUMED_TERMINATORS = {
    "U+000D CARRIAGE RETURN": "\r",
    "U+2028 LINE SEPARATOR": "\u2028",
    "U+2029 PARAGRAPH SEPARATOR": "\u2029",
    "U+0085 NEXT LINE": "\x85",
    "U+000B LINE TABULATION": "\v",
    "U+000C FORM FEED": "\f",
}

PKG_DIR = Path(charter.__file__).parent


# --------------------------------------------------------------------------- #
# the inventory: every anchored pattern in charter/, discovered rather than listed
# --------------------------------------------------------------------------- #
def _modules() -> dict[str, object]:
    """Every importable `charter.*` module, by dotted name.

    ``charter.__main__`` is skipped rather than caught: importing it RUNS the CLI, which
    parses `sys.argv` — the unittest arguments — and exits. `SystemExit` is a
    `BaseException`, so a bare ``except Exception`` would not have contained it either.
    """
    out: dict[str, object] = {"charter": charter}
    for mod in pkgutil.walk_packages([str(PKG_DIR)], prefix="charter."):
        if mod.name.rpartition(".")[2] == "__main__":
            continue
        try:
            out[mod.name] = importlib.import_module(mod.name)
        except Exception:                       # optional/pluggable backends
            continue
    return out


def _ends_in_bare_dollar(pattern: str | bytes) -> bool:
    """Does *pattern* end in a ``$`` that is a metacharacter rather than a literal?

    Decided by counting the backslashes in front of it: an odd count escapes the ``$``
    into the character itself, an even count (zero included) leaves it as the anchor.
    """
    if isinstance(pattern, bytes):
        pattern = pattern.decode("latin-1")
    return re.search(r"(?<!\\)(?:\\\\)*\$\Z", pattern) is not None


def anchored_constants() -> dict[str, re.Pattern]:
    """``{"module.CONST": pattern}`` for every module-level compiled pattern in `charter`
    that ends in a bare ``$`` and is not a MULTILINE line scanner.

    Read off the *compiled objects*, never off the source text: several of these are built
    by joining a table (`hooks._REDIRECT_RE` is ``"^\\d*(?:" + "|".join(...) + ")$"``), and
    a source-level scan for a trailing ``$`` is the same "match the spelling" mistake this
    file is about, one level up.
    """
    found: dict[str, re.Pattern] = {}
    for name, mod in _modules().items():
        for attr in vars(mod):
            val = getattr(mod, attr, None)
            if not isinstance(val, re.Pattern):
                continue
            if val.flags & re.MULTILINE:        # `$` at a line boundary is the point
                continue
            if _ends_in_bare_dollar(val.pattern):
                found[f"{name.removeprefix('charter.')}.{attr}"] = val
    return found


def _module_name(path: Path) -> str:
    """``charter/frame/tmuxctl.py`` -> ``frame.tmuxctl`` — the key `anchored_constants`
    uses, so a call site and a constant can be compared without guessing."""
    rel = path.relative_to(PKG_DIR).with_suffix("")
    parts = [p for p in rel.parts if p != "__init__"]
    return ".".join(parts)


def _import_aliases(tree: ast.AST, here: str) -> dict[str, str]:
    """``{local name: module it refers to}`` for one file's imports.

    Needed because a constant is reached across modules under whatever local name the
    importer chose — `from .. import commands` and `from . import instance as _instance`
    both appear — and resolving `commands._MCP_RULE_RE` to its owner is the difference
    between this scan finding a real call site and finding a name collision.
    """
    pkg = here.rpartition(".")[0]
    out: dict[str, str] = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.ImportFrom):
            base = node.module or ""
            if node.level:                      # `from . import x` / `from .. import x`
                up = pkg.split(".")[:len(pkg.split(".")) - (node.level - 1)] if pkg else []
                base = ".".join([p for p in up if p] + ([base] if base else []))
            base = base.removeprefix("charter.").removeprefix("charter")
            for a in node.names:
                target = f"{base}.{a.name}".strip(".") if base else a.name
                out[a.asname or a.name] = target
        elif isinstance(node, ast.Import):
            for a in node.names:
                out[a.asname or a.name.split(".")[0]] = \
                    a.name.removeprefix("charter.").removeprefix("charter")
    return out


def _owning_constant(base: ast.AST, here: str, aliases: dict[str, str]) -> str | None:
    """``"module.CONST"`` for the object a `.match(...)` was called on, or ``None``.

    ``_NAME_RE.match(x)`` in `persona.py` is `persona._NAME_RE`; `commands._MCP_RULE_RE`
    reached from `harness/opencode.py` resolves through *aliases* to `commands._MCP_RULE_RE`
    rather than to a constant of that name in the calling file.
    """
    if isinstance(base, ast.Name):
        return f"{here}.{base.id}"
    if isinstance(base, ast.Attribute) and isinstance(base.value, ast.Name):
        owner = aliases.get(base.value.id, base.value.id)
        return f"{owner}.{base.attr}" if owner else base.attr
    return None


#: **Admitters** — "is this value acceptable?". Each maps to a value the rule accepts, which
#: `TestEveryAdmitterRefusesATrailingNewline` then re-asks with a newline glued on. The
#: sample is what makes a new entry cost one line and buy a real assertion; a rule added
#: without one fails `TestTheInventoryIsClassified` rather than passing unswept.
ADMITTERS: dict[str, str] = {
    "persona._NAME_RE": "evil",
    "instance.WORKSPACE_NAME_RE": "Evil",
    "instance.CHANGE_NAME_RE": "component-api-2",
    "docsrc._TOPIC": "guide",
    "plugincache._PLUGIN_ID_RE": "charter@charter-cp",
    "plugincache._MARKETPLACE_RE": "charter-cp",
    "frame.component._ID_RE": "repos",
    "instance._HOTKEY_RE": "C-b",
    "commands._MCP_RULE_RE": "mcp__slack",
    "commands._TOOL_RULE_RE": "Bash(git status)",
    "commands_secrets._ENV_NAME": "OP_SERVICE_ACCOUNT_TOKEN",
    "commands_secrets._ENV_NAME_RE": "OP_SERVICE_ACCOUNT_TOKEN",
    "browser._VERSION": "1.2.3",
    "instance._VERSION": "1.2.3",
    "forge.registry._HOST_RE": "git.example.com",
    "recall._REL_RE": "14d",
    # A merge sha read out of `changes/log/<host>.jsonl` and handed to `git revert` and
    # `git rev-list` as argv. An admitter in the sharpest sense: `.match` would have
    # accepted `"e0c9d13\n"`, putting a newline into a git argv out of a file a hand edit
    # or a half-written append can reach.
    "commands_change._SHA_RE": "e0c9d13",
}

#: **Detectors** — "is this token one I must account for?". Over-matching makes the guard
#: fire on MORE inputs; `fullmatch` would make it fire on fewer, and every one of these is
#: reached by a security guard where fewer means fail-open. The consequence of tightening
#: each is *measured* in `TestTighteningADetectorWouldFailOpen`, so this is a recorded
#: verdict rather than a place to park a regex nobody wanted to think about.
DETECTORS: dict[str, str] = {
    "hooks._REDIRECT_RE":
        "skipping a redirection is what lets the PROGRAM be named; tightening it names "
        "'>\\n' as the program and the reader downstream is never seen",
    "hooks._REDIRECT_READ_RE":
        "names the files the SHELL opens; tightening it drops the vault out of `reads`",
    "hooks._DURATION_RE":
        "skips `timeout`'s duration so the program can be named; tightening it names "
        "'5\\n' as the program",
    "hooks._CONFIG_KEY_RE":
        "`_is_sshcommand_config_write` documents that it errs toward True — a guard that "
        "degrades to LESS coverage is the failure it exists to close",
    "hooks._GIT_CONFIG_KEY_ENV_RE":
        "same guard through the GIT_CONFIG_KEY_<n> env mechanism; `(.*)` cannot cross a "
        "newline, so `fullmatch` refuses the assignment outright and the write is missed",
    "toolgate._VERSIONED":
        "decides whether a binary is an INTERPRETER that can run another program; "
        "tightening it takes 'python3\\n' out of the wrapper class",
}

#: **Line scanners** — module-level patterns asked of one line at a time, where ``$``
#: meaning "or the newline that ends this line" is either irrelevant (the caller already
#: split or stripped) or actively required. `instance._set_key` reads with `readlines()`,
#: so its lines KEEP their newline and ``$`` is the only reason the section header is found
#: at all; that one is measured in `TestALineScannerNeedsTheNewline`.
LINE_SCANNERS: dict[str, str] = {
    "report._ATX_HEADING": "asked of `lines[0].strip()` — there is no newline left to match",
    "hooks._INDEX_LINE_RE": "asked of `.splitlines()` output — no trailing newline",
}

#: **Substitutions** — anchored patterns that never render a VERDICT. `.sub` asks where a
#: replacement applies, not whether a value is acceptable, so "admits a trailing newline" is
#: not a sentence about them. Kept in the inventory rather than filtered out of it, because
#: the way to stop this class recurring is that every anchored pattern is looked at once;
#: `TestASubstitutionIsNeverAskedForAVerdict` holds them to `.sub` so a later caller cannot
#: quietly turn one into a predicate.
SUBSTITUTIONS: dict[str, str] = {
    "tui._HIDDEN_TRAIL":
        "strips whitespace hiding behind trailing SGR; `$` reaching over a line's own "
        "newline is correct here — the escapes are still trailing",
}

#: Anchored patterns under `charter/frame/`, which #577 did not touch: that tree was being
#: edited concurrently, and a collision in a file two changes are open in is worse than a
#: latent buffering quirk. Recorded with the assessment rather than with silence, because a
#: table that lists nine and fixes seven is how this class survives a round.
#:
#: * `frame.overlay._SGR_PARTIAL` / `._CSI_PARTIAL` — "is this buffer an INCOMPLETE escape
#:   sequence?". A buffer ending ``\\x1b[<0;1\\n`` reads as partial and is held one round
#:   longer than it should be. Present, minor, not reached by a name.
#: * `frame.tmuxctl._TMUX_ENV` — parses ``$TMUX``; a trailing newline on the pid is
#:   accepted. tmux writes this variable itself, so it is not an input anyone chooses.
FRAME_DEFERRED: dict[str, str] = {
    "frame.overlay._SGR_PARTIAL": "#577 did not edit charter/frame/ — see the PR body",
    "frame.overlay._CSI_PARTIAL": "#577 did not edit charter/frame/ — see the PR body",
    "frame.tmuxctl._TMUX_ENV": "#577 did not edit charter/frame/ — see the PR body",
}


def _classified() -> set[str]:
    """Every constant somebody has looked at and filed. The four maps are disjoint by
    intent; `TestTheInventoryIsClassified` holds them to that too, so a constant cannot be
    both an admitter and an exemption depending on which map a reader reaches first."""
    return (set(ADMITTERS) | set(DETECTORS) | set(LINE_SCANNERS)
            | set(SUBSTITUTIONS) | set(FRAME_DEFERRED))


class TestWhatPythonsDollarActuallyIs(unittest.TestCase):
    """Measured, not recalled. The whole fix rests on WHICH codepoints the engine treats as
    "the end", and getting that wrong in either direction writes a normalisation pass nobody
    needs or leaves a hole the sweep claims to have closed."""

    def test_exactly_one_codepoint_is_special_to_dollar(self):
        """1,114,112 codepoints, one answer: U+000A and nothing else."""
        rx = re.compile(r"^[a-z]+$")
        extra = {chr(cp) for cp in range(0x110000)
                 if rx.match("abc" + chr(cp)) and chr(cp) not in string.ascii_lowercase}
        self.assertEqual(extra, {LF})

    def test_nothing_else_people_assume_is_a_terminator(self):
        """The five near-misses, by name, so a failure says which belief was wrong."""
        rx = re.compile(r"^[a-z]+$")
        for label, ch in ASSUMED_TERMINATORS.items():
            with self.subTest(codepoint=label):
                self.assertIsNone(rx.match("abc" + ch))

    def test_capital_Z_is_the_end_and_dollar_is_not(self):
        """``\\Z`` is the other spelling of the property, for a pattern that must stay a
        `.match`. Held here so the sentence in the issue is a fact and not a plan."""
        self.assertIsNone(re.compile(r"^[a-z]+\Z").match("abc" + LF))
        self.assertIsNotNone(re.compile(r"^[a-z]+$").match("abc" + LF))

    def test_fullmatch_closes_it_without_touching_the_pattern(self):
        """Why the fix is one word per call site and no pattern was rewritten: `fullmatch`
        requires the pattern to CONSUME the whole string, and ``$`` consumes nothing — so a
        leftover ``^…$`` is redundant under `fullmatch`, never a hole."""
        rx = re.compile(r"^[a-z]+$")
        self.assertIsNone(rx.fullmatch("abc" + LF))
        self.assertIsNotNone(rx.fullmatch("abc"))

    def test_multiline_is_a_different_question_and_is_excluded(self):
        """`anchored_constants` skips MULTILINE patterns. That is not an oversight being
        papered over: under MULTILINE ``$`` matching at every line boundary is the entire
        reason the flag was passed, so those are not instances of this defect."""
        rx = re.compile(r"^[a-z]+$", re.MULTILINE)
        self.assertIsNotNone(rx.match("abc" + LF + "def"))


class TestTheInventoryIsClassified(unittest.TestCase):
    """The hook that makes a TENTH regex fail.

    Every anchored pattern in the package must be named in exactly one of the four maps
    above. Adding one and forgetting it is a red test with the constant's own name in the
    message — which is the only version of this that survives the next contributor.
    """

    def test_every_anchored_pattern_is_accounted_for(self):
        found = set(anchored_constants())
        classified = _classified()
        missing = sorted(found - classified)
        self.assertEqual(missing, [], (
            "a new anchored `^…$` pattern is not classified. `$` matches before a trailing "
            "newline, so decide what this one is: an ADMITTER (ask it with `fullmatch`, and "
            "add a sample value here) or a DETECTOR (over-matching is load-bearing — say "
            "why, and measure it)."))

    def test_nothing_is_classified_that_no_longer_exists(self):
        """The other direction, so a fixed or deleted pattern does not leave a reason behind
        that stops being true. A stale exemption is how an allowlist becomes a lie."""
        found = set(anchored_constants())
        stale = sorted(_classified() - found)
        self.assertEqual(stale, [])

    def test_the_four_categories_are_disjoint(self):
        """A constant in two maps reads as whichever the reader found first, and the one
        they did not find is where the next instance hides."""
        maps = [("ADMITTERS", ADMITTERS), ("DETECTORS", DETECTORS),
                ("LINE_SCANNERS", LINE_SCANNERS), ("SUBSTITUTIONS", SUBSTITUTIONS),
                ("FRAME_DEFERRED", FRAME_DEFERRED)]
        for i, (a_name, a) in enumerate(maps):
            for b_name, b in maps[i + 1:]:
                with self.subTest(pair=f"{a_name}/{b_name}"):
                    self.assertEqual(set(a) & set(b), set())

    def test_a_substitution_is_never_asked_for_a_verdict(self):
        """`SUBSTITUTIONS` is exempt because `.sub` renders no verdict. That is only true
        while it stays a `.sub`, so this is the condition of the exemption rather than a
        note beside it."""
        wrong = []
        for path in sorted(PKG_DIR.rglob("*.py")):
            tree = ast.parse(path.read_text(), str(path))
            here = _module_name(path)
            aliases = _import_aliases(tree, here)
            for node in ast.walk(tree):
                if not (isinstance(node, ast.Call)
                        and isinstance(node.func, ast.Attribute)
                        and node.func.attr in ("match", "search", "fullmatch")):
                    continue
                owner = _owning_constant(node.func.value, here, aliases)
                if owner in SUBSTITUTIONS:
                    wrong.append(f"{path.name}:{node.lineno} {owner}.{node.func.attr}()")
        self.assertEqual(wrong, [])

    def test_the_sweep_finds_the_ones_the_issue_named(self):
        """A discovery sweep that discovers nothing passes silently. This is its control:
        the nine from #577 must be in the inventory it built."""
        found = set(anchored_constants())
        for name in ("persona._NAME_RE", "instance.WORKSPACE_NAME_RE", "docsrc._TOPIC",
                     "plugincache._PLUGIN_ID_RE", "plugincache._MARKETPLACE_RE",
                     "frame.component._ID_RE", "instance._HOTKEY_RE",
                     "commands._MCP_RULE_RE", "commands_secrets._ENV_NAME"):
            with self.subTest(constant=name):
                self.assertIn(name, found)


class TestEveryAdmitterRefusesATrailingNewline(unittest.TestCase):
    """The property, asked of every admitter at once rather than of the two in the title of
    the issue. A tenth admitter added to the map above is swept by this without anybody
    writing a test for it."""

    def test_the_sample_is_actually_admitted(self):
        """The control. A sample that the rule refuses outright would make every assertion
        below pass for the wrong reason — which is how #579's first table read 17/17."""
        pats = anchored_constants()
        for name, sample in ADMITTERS.items():
            with self.subTest(constant=name):
                self.assertIsNotNone(pats[name].fullmatch(sample),
                                     f"{name} does not admit its own sample {sample!r}")

    def test_a_trailing_newline_is_refused(self):
        pats = anchored_constants()
        for name, sample in ADMITTERS.items():
            with self.subTest(constant=name):
                self.assertIsNone(pats[name].fullmatch(sample + LF))

    def test_so_is_every_other_way_a_newline_can_sit_in_a_name(self):
        """Leading, doubled and embedded — because "ends in a newline" is the instance and
        "holds a newline" is the property. `.match` never refused the trailing one; the
        others it refused for a different reason, and the fix must not have traded them."""
        pats = anchored_constants()
        for name, sample in ADMITTERS.items():
            for label, bad in (("trailing", sample + LF),
                               ("leading", LF + sample),
                               ("doubled", sample + LF + LF),
                               ("embedded", sample[:1] + LF + sample[1:]),
                               ("crlf", sample + "\r" + LF),
                               ("cr", sample + "\r")):
                with self.subTest(constant=name, shape=label):
                    self.assertIsNone(pats[name].fullmatch(bad))

    def test_the_public_predicates_agree_with_their_patterns(self):
        """A pattern is not a rule until a caller asks it. These are the functions the rest
        of charter actually calls — the layer where #577 was reachable, and the layer a fix
        applied to the constant alone would have missed."""
        cases = [
            ("persona.valid_name", persona.valid_name, "evil"),
            ("persona.reference_ok", persona.reference_ok, "evil"),
            ("instance.workspace_name_ok", instance.workspace_name_ok, "evil"),
            ("browser.version_ok", browser.version_ok, "1.2.3"),
            ("instance.version_ok", instance.version_ok, "1.2.3"),
            ("forge.registry.host_ok", registry.host_ok, "git.example.com"),
            ("frame.component.usable_id", component.usable_id, "repos"),
        ]
        for label, fn, good in cases:
            with self.subTest(predicate=label):
                self.assertTrue(fn(good), f"{label} refused its own sample")
                self.assertFalse(fn(good + LF), f"{label} still admits a trailing newline")

    def test_persona_says_WHY_rather_than_only_no(self):
        """#361's rule, applied to the new refusal: the verdict and the sentence an operator
        reads must describe the same failure. A silent False here would send somebody
        looking for a persona directory that is on disk and looks right."""
        refusal = persona.reference_refusal("evil" + LF)
        self.assertIsNotNone(refusal)
        self.assertNotIn(LF, refusal)

    def test_topics_and_plugin_ids_refuse_it_too(self):
        """The two the issue marked "not checked" and the one it marked "names no file".

        `docs show` carries its own control: the topic WITHOUT the newline really is a page
        on disk, so the None below is the newline being refused and not `source()` being
        absent, which is how this assertion would otherwise pass on an empty tree."""
        topic = docsrc.topics()[0]
        self.assertIsNotNone(docsrc.read(topic))
        self.assertIsNone(docsrc.read(topic + LF))
        self.assertNotIn(topic + LF, docsrc.topics())
        self.assertIsNone(plugincache.marketplace_clone("charter-cp" + LF))
        self.assertIsNone(plugincache.refresh_argvs("charter@charter-cp" + LF, "user"))

    def test_an_env_name_with_a_newline_cannot_reach_a_dotenv_line(self):
        """The tenth, which #577's table does not list: `commands_secrets._ENV_NAME_RE`
        guards the name written into a dotenv file, and its caller does NOT strip."""
        with self.assertRaises(ValueError):
            commands_secrets._dotenv_line("FOO" + LF, "x")


class TestCallerSideStripIsNotTheGuard(unittest.TestCase):
    """Two of the nine were correct only because somebody remembered a `.strip()` at the
    call site. That is a caller compensating, not the rule being right — the eleventh caller
    inherits nothing. The rules are now right on their own, and the strips STAY, because
    each is normalising the value that is then USED and not merely the one that is checked.
    """

    def test_the_rule_holds_without_the_strip(self):
        self.assertIsNone(commands._MCP_RULE_RE.fullmatch("mcp__slack" + LF))
        self.assertIsNone(commands_secrets._ENV_NAME.fullmatch("FOO" + LF))

    def test_the_strip_still_does_its_own_job(self):
        """`_as_rule` RETURNS `p`, so the strip decides what is written into the settings
        file — remove it and charter stores a rule with spaces around it that the host then
        does not match. Pinned so "the regex is right now" does not read as "the strip is
        redundant now"."""
        self.assertEqual(commands._as_rule("  Bash(git status)  "), "Bash(git status)")
        self.assertEqual(commands._as_rule("  mcp__slack  "), "mcp__slack")

    def test_a_padded_name_is_still_refused_where_nothing_strips(self):
        """The other half: where no caller strips, surrounding space is not silently
        forgiven either. `valid_name` never admitted a space — this states it, so a later
        "be forgiving" change has to argue with a test."""
        for bad in (" evil", "evil ", "evil\t", " evil "):
            with self.subTest(name=bad):
                self.assertFalse(persona.valid_name(bad))


class TestTighteningADetectorWouldFailOpen(unittest.TestCase):
    """The measurement behind `DETECTORS`.

    Every case here runs the REAL guard twice — as shipped, and with the constant's `.match`
    rebound to `.fullmatch` — and asserts the shipped one catches what the tightened one
    misses. Without this the reasons in `DETECTORS` are assertions in a comment, and the
    next sweep "finishes" #577 by turning five security guards fail-open.
    """

    class _Tightened:
        """A compiled pattern whose `.match` is its `.fullmatch` — the change not made."""

        def __init__(self, rx):
            self._rx = rx

        def match(self, s, *a, **k):
            return self._rx.fullmatch(s, *a, **k)

        def __getattr__(self, name):
            return getattr(self._rx, name)

    def tighten(self, mod, attr):
        original = getattr(mod, attr)
        setattr(mod, attr, self._Tightened(original))
        self.addCleanup(setattr, mod, attr, original)

    VAULT = ".charter/vaults/x.json"

    def test_a_redirection_that_opens_a_vault_stays_visible(self):
        toks = ["<" + LF, self.VAULT, "true"]
        self.assertEqual(hooks._redirect_reads(toks), [self.VAULT])
        self.tighten(hooks, "_REDIRECT_READ_RE")
        self.assertEqual(hooks._redirect_reads(toks), [],
                         "if this ever equals the shipped answer, _REDIRECT_READ_RE may "
                         "move to ADMITTERS")

    def test_a_leading_redirection_is_still_skipped_so_the_program_is_named(self):
        toks = [">" + LF, "f", "cat", self.VAULT]
        self.assertEqual(hooks._split_env_chdir(toks)[0], "cat")
        self.tighten(hooks, "_REDIRECT_RE")
        self.assertEqual(hooks._split_env_chdir(toks)[0], ">" + LF)

    def test_a_timeout_duration_is_still_skipped(self):
        toks = ["timeout", "5" + LF, "cat", self.VAULT]
        self.assertEqual(hooks._split_env_chdir(toks)[0], "cat")
        self.tighten(hooks, "_DURATION_RE")
        self.assertEqual(hooks._split_env_chdir(toks)[0], "5" + LF)

    def test_an_sshcommand_config_write_is_still_caught(self):
        args = ["config", "core.sshCommand" + LF, "ssh -i /tmp/k"]
        self.assertTrue(hooks._is_sshcommand_config_write(args))
        self.tighten(hooks, "_CONFIG_KEY_RE")
        self.assertFalse(hooks._is_sshcommand_config_write(args))

    def test_the_env_form_of_that_write_is_still_caught(self):
        env = ["GIT_CONFIG_KEY_1=core.sshCommand" + LF]
        self.assertTrue(hooks._has_git_config_env_sshcommand(env))
        self.tighten(hooks, "_GIT_CONFIG_KEY_ENV_RE")
        self.assertFalse(hooks._has_git_config_env_sshcommand(env))

    def test_an_interpreter_is_still_recognised(self):
        self.assertTrue(toolgate._is_interpreter("python3" + LF))
        self.tighten(toolgate, "_VERSIONED")
        self.assertFalse(toolgate._is_interpreter("python3" + LF))


class TestTheRefusalsThisFixMadeReachable(unittest.TestCase):
    """Tightening a rule turns "accepted" into "refused", and a refusal is a line of
    charter's own output NAMING the offender. So each rule tightened here has a sentence
    that nobody could reach with a newline before, and can now.

    `persona.reference_refusal` is fixed in this change: it is the sentence #577 quotes
    `mcpseen.label` about, `contain` was already imported, and the branch three lines above
    it already went through `contain.path_sentence`.

    **The version refusals are NOT fixed here, and that is recorded rather than hidden.**
    `instance.NOT_A_VERSION` and `browser.NOT_A_VERSION` are `.format`-ed at six sites
    across five modules, and containing them properly means the `contain.refusal` pattern —
    a template constant plus a module-level renderer — in two more modules. That is #453's
    surface sweep, not #577's regex, and doing it inside a change to ten regexes would put
    it in a diff nobody reviewing either question would look for it in.

    These assertions therefore pin what charter does TODAY, including the part that is
    wrong. They are written to fail the day somebody fixes it, so the fix arrives with this
    note attached instead of silently disagreeing with it.
    """

    def test_the_persona_refusal_is_contained(self):
        self.assertNotIn(LF, persona.reference_refusal("evil" + LF))

    def test_the_version_refusals_are_not_yet(self):
        """Known gap. When this goes red, delete it and the paragraph above: the follow-up
        landed. `util.err` prints its message raw (`print("✗ " + msg)`), so the newline in
        here really does write a second line of charter's stderr."""
        for label, template, value in (
                ("instance", instance.NOT_A_VERSION, "1.2.3" + LF),
                ("browser", browser.NOT_A_VERSION, "1.2.3" + LF)):
            with self.subTest(module=label):
                self.assertIn(LF, template.format(version=value))

    def test_and_the_dotenv_name_refusal_is_not_yet_either(self):
        """Same gap, third surface — and the one this change made reachable at all, since
        `_dotenv_line` returned a line rather than raising before #577."""
        with self.assertRaises(ValueError) as e:
            commands_secrets._dotenv_line("FOO" + LF, "x")
        self.assertIn(LF, str(e.exception))


class TestALineScannerNeedsTheNewline(unittest.TestCase):
    """The third category, and the reason a blanket "no `$` with `.match`" rule would be
    wrong. `instance._set_key` reads `charter.toml` with `readlines()`, so every line it
    scans KEEPS its newline — ``$`` matching before it is the only reason the section header
    is found. Function-local, so it is not in `anchored_constants`; measured here so the
    exemption is a fact rather than a claim."""

    def test_the_toml_section_header_is_found_with_its_newline_attached(self):
        header = re.compile(r"^[ \t]*\[workspace\][ \t]*$")
        self.assertIsNotNone(header.match("[workspace]" + LF))
        self.assertIsNone(header.fullmatch("[workspace]" + LF))

    def test_and_the_edit_it_guards_still_works(self):
        """End to end, so the paragraph above is load-bearing rather than decorative."""
        root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        (root / "charter.toml").write_text('schema = 1\n[workspace]\ndefault = "a"\n')
        self.assertTrue(instance._set_key(root, "workspace", "default", "b"))
        self.assertIn('default = "b"', (root / "charter.toml").read_text())


class TestAPersonaNamedWithANewlineNoLongerLoads(PersonaIso):
    """The end of the chain, on disk. The regex is the cause; THIS is the defect — a
    directory `personas/evil<LF>/` that resolved, loaded, and was rendered into a generated
    agent's YAML frontmatter with a blank line through it (#577, #453).

    Asserted against a directory that really exists, because "the predicate returns False"
    is a fact about a function and this is a fact about the plane.
    """

    NAME = "evil" + LF

    def plant(self) -> Path:
        """A `personas/evil<LF>/` that is a real directory with a real `persona.md` — the
        thing the issue reproduced, not a string standing in for it."""
        d = config.PERSONAS_DIR / self.NAME
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text(
            "---\nname: evil\nrole: Victim\n"
            'description: "The Victim persona."\n---\n\n# evil\n\ncharter body\n')
        self.assertTrue(d.is_dir())                       # the fixture is real
        return d

    def test_it_is_not_a_name_and_does_not_load(self):
        self.plant()
        self.assertFalse(persona.valid_name(self.NAME))
        self.assertFalse(persona.reference_ok(self.NAME))
        self.assertIsNone(persona.load(self.NAME))

    def test_sync_agents_writes_no_agent_for_it(self):
        """The defect itself. `cmd_persona_sync_agents` iterates `list_personas()`, which
        does NOT ask `valid_name` — so the refusal has to bite at `load`, and asserting the
        predicate alone would have proved nothing about the file on disk.

        Measured end to end for that reason: on `main` this wrote
        ``.claude/agents/evil<LF>.md`` whose frontmatter carried ``name: evil`` followed by
        a blank line, and whose `GENERATED by` comment was split across two physical lines
        by a path that is one name.
        """
        self.plant()
        self.make_persona("steward")
        commands_persona.cmd_persona_sync_agents(
            SimpleNamespace(persona=None, approve_mcp=False, yes=True, dry_run=False))
        agents = config.ROOT / ".claude" / "agents"
        written = sorted(p.name for p in agents.iterdir()) if agents.exists() else []
        self.assertEqual(written, ["steward.md"])

    def test_the_roster_still_SHOWS_it_so_lint_can_name_it(self):
        """The deliberate half, pinned so a later "tidy up" does not filter `list_personas`
        by `valid_name` and call it a fix. The roster answers "what is on disk"; `lint`
        answers "and this one is not a persona". Dropping it from the roster would make the
        directory invisible instead of reported — which is #498's defect, not its fix.
        """
        self.plant()
        self.assertIn(self.NAME, persona.list_personas())

    def test_the_lint_row_that_names_it_is_still_one_line(self):
        """And the row naming it does not get to write a second row (#453, #498)."""
        self.plant()
        buf = io.StringIO()
        with redirect_stdout(buf), redirect_stderr(buf):
            commands_persona.cmd_persona_lint(SimpleNamespace(name=None, strict=False))
        rows = [r for r in buf.getvalue().splitlines() if "evil" in r]
        self.assertEqual(len(rows), 1, buf.getvalue())
        self.assertIn("evil", rows[0])
        self.assertNotIn(LF, rows[0])

    def test_an_ordinary_persona_beside_it_is_untouched(self):
        """The control, and the answer to "who does this refusal cost?". A name charter
        minted is unaffected; only the one no `persona create` could have produced is."""
        self.make_persona("steward")
        self.plant()
        self.assertIn("steward", persona.list_personas())
        self.assertTrue(persona.valid_name("steward"))
        self.assertIsNotNone(persona.load("steward"))


class TestNoAdmitterIsStillAskedWithMatch(unittest.TestCase):
    """The static half. `TestEveryAdmitterRefusesATrailingNewline` proves the PATTERN is
    right; this proves every CALL SITE asks it the right way, which is where #577 lived —
    the patterns were always fine, `.match` was the bug.

    Source-level on purpose: a call site on a branch no test reaches is exactly the tenth
    caller the issue is about, and only reading the source finds it.
    """

    def test_every_call_site_of_an_admitter_uses_fullmatch(self):
        offenders = []
        for path in sorted(PKG_DIR.rglob("*.py")):
            tree = ast.parse(path.read_text(), str(path))
            here = _module_name(path)
            aliases = _import_aliases(tree, here)
            for node in ast.walk(tree):
                if not (isinstance(node, ast.Call)
                        and isinstance(node.func, ast.Attribute)
                        and node.func.attr in ("match", "search")):
                    continue
                owner = _owning_constant(node.func.value, here, aliases)
                if owner in ADMITTERS:
                    offenders.append(
                        f"{path.relative_to(PKG_DIR.parent)}:{node.lineno} "
                        f"{owner}.{node.func.attr}()")
        self.assertEqual(offenders, [], (
            "an anchored `^…$` admitter is being asked with `.match`/`.search`, which "
            "matches before a trailing newline. Use `.fullmatch`."))

    def test_the_scan_resolves_the_module_and_not_just_the_name(self):
        """Its own control, because the first version of this scan keyed on the bare
        constant name and reported `charter/frame/tmuxctl.py:329 _VERSION.match()` — a
        DIFFERENT `_VERSION` (``^tmux (\\d+)\\.(\\d+)``, no trailing ``$``) that shares a
        name with `browser._VERSION` and `instance._VERSION`. A scan that cannot tell three
        constants apart is the same "matched the spelling" mistake this file is about."""
        tmuxctl = PKG_DIR / "frame" / "tmuxctl.py"
        tree = ast.parse(tmuxctl.read_text(), str(tmuxctl))
        here = _module_name(tmuxctl)
        aliases = _import_aliases(tree, here)
        resolved = {_owning_constant(n.func.value, here, aliases)
                    for n in ast.walk(tree)
                    if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute)
                    and n.func.attr in ("match", "search")}
        self.assertIn("frame.tmuxctl._VERSION", resolved)
        self.assertNotIn("browser._VERSION", resolved)
        self.assertNotIn("instance._VERSION", resolved)


if __name__ == "__main__":
    unittest.main()
