"""A guard with no documented override is a guard people uninstall (#370).

A hook `deny` is the strongest thing charter does to a session: no permission mode lifts it,
`charter guard` cannot relax it, and it says so. Every denial named a remedy for the workflow
the operator was *supposed* to be doing, and not one named what to do when the guard is
simply wrong about this case. Nothing else named it either — no config key, no environment
variable, no per-guard switch anywhere in `charter/config.py`.

So the only route past a denial charter got wrong was to delete the hook from
`.claude/settings.json` or disable the plugin, which removes every guard, both injections and
all the tallies together. Nuclear, and undiscoverable at the moment it is needed. Every guard
is eventually wrong about something, and the response a design invites at that moment is the
response it gets: when the only move is nuclear, the guard that was wrong once is off for
ever, along with the ones that were not.

**The ruling this file pins.** The override is that you run the command yourself, outside
the agent — and there is deliberately no switch charter can read. That is not a dodge:

* charter's guards exist because committed data must not reach a credential or make
  something run. A key in `charter.toml` would be a key a committed file could flip, and an
  environment variable sits on a command line the agent writes. An override charter can read
  is an override the AGENT controls, which is exactly the party being bound.
* A `PreToolUse` hook governs the harness's tools. The operator's own shell was never on
  that side of the line, so running it there works around nothing.

**Appended in `_deny`, not at the call sites**, which is the assertion with the most
value here: the next guard added carries the override without anyone remembering to,
and the trace tally keys — computed from the reason BEFORE it reaches `_deny` — cannot drift.
#710, #778, the state-write guard and the chat handoff's own A7 each arrived without a row in
`DENIALS` below, and each carried the note regardless. The README's count of them lagged the
same way, and so, for the state-write guard, did its entry in `docs/hooks.md` — three
hand-kept lists, fixed by hand three times (#1000).

**So the set of guards is measured, not remembered.** A guard is a `_deny` call site in
`charter/hooks.py` — `test_deny_is_the_only_thing_that_emits_a_denial` is what makes that the
whole set — and :class:`TestTheGuardsAreTheDenyCallSites` reads those sites out of the source,
drives every row through a spy on `_deny`, and fails naming the line of any site no row reaches.
A new guard fails this file until it has a row that denies through it, an entry under
`## The guards` the row names, and the README's count moved to match.
"""

from __future__ import annotations

import ast
import re
import subprocess
import sys
import unittest
from pathlib import Path
from typing import Callable, NamedTuple
from unittest import mock

from tests._isolation import PersonaIso, run_hook
from tests.test_hooks import InAControlPlane
from charter import config, hooks, trace

REPO = Path(__file__).resolve().parents[1]
DOC = REPO / "docs" / "hooks.md"
README = REPO / "README.md"
SECTION = "## When a guard is wrong"
GUARDS_SECTION = "## The guards"


class Denial(NamedTuple):
    #: The guard's bold lead-in under `## The guards` in `docs/hooks.md`, spelled by hand — a
    #: title read out of the page would agree with whatever the page says.
    entry: str
    handler: Callable[[], int]
    payload: dict


#: Every denial charter can emit, driven end to end through the real handler. Which rows must
#: exist is not this table's say: `TestTheGuardsAreTheDenyCallSites` holds it to the source.
DENIALS = {
    "secret-leak": Denial("Secret leak", hooks.pretooluse,
                          {"tool_input": {"command": "charter secret get v K --reveal"}}),
    "vault-read": Denial("Vault read", hooks.pretooluse_read,
                         {"tool_name": "Read",
                          "tool_input": {"file_path": ".charter/vaults/d.json"}}),
    "single-credential": Denial("One credential", hooks.pretooluse,
                                {"tool_input": {"command": "git clone git@github.com:a/b.git"}}),
    "plane-root-branch": Denial("Plane-root branch move", hooks.pretooluse,
                                {"tool_input": {"command": "git checkout -b feature"}}),
    "plane-root-reset": Denial("Plane-root history wipe", hooks.pretooluse,
                               {"tool_input": {"command": "git reset --hard origin/main"}}),
    "release-floor": Denial("Release floor", hooks.pretooluse,
                            {"tool_input": {"command": "gh release create v9.9.9"},
                             "permission_mode": "bypassPermissions"}),
    "forge-substitution": Denial("Forge body substitution", hooks.pretooluse,
                                 {"tool_input": {"command":
                                                 'gh issue create --body "a `id -un` span"'}}),
    "charter-substitution": Denial("charter's own text substitution", hooks.pretooluse,
                                   {"tool_input": {"command":
                                                   'charter persona remember "a `id -un` span"'}}),
    # A7's spelling refusal is the cheapest of its four to reach — no `agent_id`, no mode, no
    # stdin to arrange. One row for four refusals, because they are one `_deny` call site.
    "handoff": Denial("A handoff the prompt cannot stand in front of", hooks.pretooluse,
                      {"tool_input": {"command": "python3 -m charter handoff beta"}}),
    # Denies on `Write`/`Edit` rather than on Bash, which is how it went missing from all three
    # lists at once: nobody looking at the Bash handler saw it.
    "state-write": Denial("A hand-written state file", hooks.pretooluse_edit,
                          {"tool_name": "Write",
                           "tool_input": {"file_path": ".charter/persona"}}),
}


def _deny_call_sites() -> dict[range, str]:
    """Every `_deny(...)` call in `charter/hooks.py`: the lines it spans, and its first line.

    Read from the AST, not by a regex over the text, so a call split across lines or spelled
    with different spacing is still one site."""
    src = Path(hooks.__file__).read_text()
    lines = src.splitlines()
    return {range(n.lineno, n.end_lineno + 1): lines[n.lineno - 1].strip()
            for n in ast.walk(ast.parse(src))
            if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == "_deny"}


#: English for a count, as the README spells one. A count past the end of this map fails the
#: README test with a message, rather than being read as some other number.
_NUMBER_WORDS = {w: i for i, w in enumerate(
    "zero one two three four five six seven eight nine ten eleven twelve thirteen fourteen "
    "fifteen sixteen seventeen eighteen nineteen twenty".split())}


def _root_ahead_of_its_upstream(case) -> None:
    """Give the tmp plane a real upstream and one commit that never reached it.

    The only denial here whose condition is a fact about the WORLD rather than about the
    command: the reset guard measures whether commits would actually be destroyed, and
    stands aside when they would not. A bare tmp plane is that "would not", so without this
    the fixture would land in the `assertIsNotNone` above as "never reached the guard" —
    which is exactly the failure the precondition assertion exists to name.
    """
    root = Path(config.ROOT)
    if (root / ".git").exists():
        return
    remote = root / "origin.git"

    def git(*args, cwd=root):
        subprocess.run(["git", "-C", str(cwd), *args], check=True, capture_output=True)

    subprocess.run(["git", "init", "-q", "--bare", "-b", "main", str(remote)],
                   check=True, capture_output=True)
    subprocess.run(["git", "init", "-q", "-b", "main", str(root)],
                   check=True, capture_output=True)
    for k, v in (("commit.gpgsign", "false"), ("user.email", "t@e"), ("user.name", "t")):
        git("config", k, v)
    git("remote", "add", "origin", str(remote))
    (root / "README").write_text("plane\n")
    git("add", "-A")
    git("commit", "-qm", "init")
    git("push", "-q", "-u", "origin", "main")
    (root / "memory.md").write_text("learned\n")
    git("add", "-A")
    git("commit", "-qm", "memory: one")


#: Fixture work a denial needs before its command is refusable. Keyed rather than folded
#: into `setUp` so each guard's precondition stays visible next to the guard.
SETUP = {"plane-root-reset": _root_ahead_of_its_upstream}


class DenialCase(InAControlPlane):
    def reason(self, name: str, sid: str = "s") -> str:
        row = DENIALS[name]
        SETUP.get(name, lambda _case: None)(self)
        r = run_hook(row.handler, {"cwd": str(self.tmp), "session_id": sid, **row.payload})
        self.assertIsNotNone(r, f"{name}: fixture never reached the guard")
        out = r["hookSpecificOutput"]
        self.assertEqual("deny", out["permissionDecision"],
                         f"{name}: precondition — this fixture must DENY, not {out}")
        return out["permissionDecisionReason"]


class TestEveryDenialNamesTheOverride(DenialCase):
    def test_every_one_of_them(self):
        """No count here: which guards exist is `TestTheGuardsAreTheDenyCallSites`' question,
        and a number typed into this test is the list that lagged."""
        for name in DENIALS:
            with self.subTest(guard=name):
                self.assertIn(hooks._OVERRIDE_NOTE, self.reason(name))


class TestTheGuardsAreTheDenyCallSites(DenialCase):
    """#1000. The enumeration above is held to the guards that exist, not to memory.

    The expected set comes from the source — every `_deny` call site in `charter/hooks.py` —
    and the observed set from running every row with `_deny` spied on, so the two halves are
    independent: a row cannot vouch for a guard it never reaches, and a guard no row reaches
    is named by its line."""

    def _site_each_row_reaches(self) -> dict[str, list[range]]:
        sites = _deny_call_sites()
        here = Path(hooks.__file__).resolve()
        real = hooks._deny
        reached: list[tuple[Path, int]] = []

        def spy(event: str, reason: str) -> int:
            # Recorded, not asserted: a handler's own `except Exception` would swallow an
            # assertion raised in here and leave a message about something else.
            caller = sys._getframe(1)
            reached.append((Path(caller.f_code.co_filename).resolve(), caller.f_lineno))
            return real(event, reason)

        out: dict[str, list[range]] = {}
        with mock.patch.object(hooks, "_deny", spy):
            for name in DENIALS:
                reached.clear()
                self.reason(name, sid=f"site-{name}")
                self.assertEqual([here] * len(reached), [f for f, _ in reached],
                                 f"{name}: `_deny` called from outside charter/hooks.py")
                out[name] = [span for span in sites for _f, line in reached if line in span]
        return out

    def test_every_guard_has_a_row_that_denies_through_it(self):
        sites = _deny_call_sites()
        self.assertTrue(sites, "precondition: no `_deny` call site was found in hooks.py")
        reached = self._site_each_row_reaches()
        for name, spans in reached.items():
            with self.subTest(row=name):
                self.assertEqual(1, len(spans),
                                 f"{name}: a row must deny through exactly one guard")
        covered = {spans[0] for spans in reached.values() if len(spans) == 1}
        missing = [f"hooks.py:{span.start}: {sites[span]}" for span in sites
                   if span not in covered]
        self.assertEqual(
            [], missing,
            "a guard with no row in DENIALS — add one whose payload this guard denies, name its "
            "entry under `## The guards` in docs/hooks.md, and move the README's count")

    def test_no_two_rows_are_the_same_guard(self):
        """A second row for a guard that already has one would make the count look right while
        the new guard still has none."""
        firsts = [spans[0].start for spans in self._site_each_row_reaches().values() if spans]
        self.assertEqual(len(firsts), len(set(firsts)), "two rows deny through one guard")

    def test_deny_is_only_ever_called_by_name(self):
        """What makes the call sites the whole set: a `_deny` bound to another name, or passed
        along as a value, would be a guard this reading cannot see."""
        tree = ast.parse(Path(hooks.__file__).read_text())
        called = {id(n.func) for n in ast.walk(tree)
                  if isinstance(n, ast.Call) and isinstance(n.func, ast.Name)
                  and n.func.id == "_deny"}
        other = [n.lineno for n in ast.walk(tree)
                 if isinstance(n, ast.Name) and n.id == "_deny" and id(n) not in called]
        self.assertEqual([], other, "`_deny` referenced without being called")


class TestEveryGuardIsDocumented(unittest.TestCase):
    """The README pointed at `docs/hooks.md` as "the five guards" while ten denied, and the page
    had no entry for the state-write guard (#999). Both are held to the rows here, and the rows
    are held to the source."""

    def test_each_row_is_an_entry_under_the_guards_heading_and_nothing_else_is(self):
        body = DOC.read_text().split(f"\n{GUARDS_SECTION}\n", 1)[1].split("\n## ", 1)[0]
        entries = [m.group(1).rstrip(".") for m in re.finditer(r"^- \*\*(.+?)\*\*", body, re.M)]
        self.assertEqual(sorted(row.entry for row in DENIALS.values()), sorted(entries))

    def test_the_readme_counts_the_guards_the_source_has(self):
        m = re.search(r"\bthe (\w+) guards that deny\b", README.read_text())
        self.assertIsNotNone(m, "README.md no longer says how many guards deny")
        self.assertIn(m.group(1), _NUMBER_WORDS, f"README.md spells the count {m.group(1)!r}")
        self.assertEqual(len(_deny_call_sites()), _NUMBER_WORDS[m.group(1)],
                         "README.md's count of the guards that deny is not the source's")


class TestWhatTheOverrideActuallySays(DenialCase):
    """The wording is the deliverable. A pointer to a section that answers nothing, or a
    hint that reads as "ask charter nicely", would leave the operator exactly where #370
    found them."""

    def setUp(self) -> None:
        super().setUp()
        self.note = hooks._OVERRIDE_NOTE

    def test_it_says_there_is_no_switch(self):
        self.assertRegex(self.note, r"no .*(config|switch)")

    def test_it_says_why_there_is_no_switch(self):
        """Not a refusal — a reason. "A file could flip it" is the whole argument."""
        self.assertIn("committed", self.note)

    def test_it_names_the_move_that_works(self):
        self.assertIn("your own terminal", self.note)

    def test_it_points_at_the_section_that_explains_it(self):
        self.assertIn("docs/hooks.md", self.note)
        self.assertIn("When a guard is wrong", self.note)

    def test_it_does_not_read_as_a_bypass_the_agent_can_take(self):
        """An override an agent can act on is not an override, it is a hole. The note must
        not name a flag, variable or command that would lift the denial."""
        for bait in ("CHARTER_", "--force", "--no-verify", "charter guard allow"):
            self.assertNotIn(bait, self.note)


class TestTheTallyKeysAreUnchanged(DenialCase):
    """The trace `reason` is a tally key, and two guards derive it from the first 70
    characters of their prose. Appending in `_deny` keeps that prefix bit-identical;
    prepending would have silently restarted every series in every existing store."""

    KEYS = {"secret-leak": "would reveal a secret value into the conversation (--reveal)",
            "vault-read": "reads a vault/secret file directly (would print plaintext)",
            "single-credential": "single-credential",
            "plane-root-branch": "plane-root-branch",
            "plane-root-reset": "plane-root-reset",
            "release-floor": "release-floor"}

    def test_each_guard_still_traces_the_key_it_always_did(self):
        for name, key in self.KEYS.items():
            with self.subTest(guard=name):
                sid = f"s-{name}"
                self.reason(name, sid=sid)
                rows = [e for e in trace.read(sid) if e.get("event") == "deny"]
                self.assertEqual(1, len(rows), f"{name}: precondition — one deny row")
                self.assertTrue(rows[0]["reason"].startswith(key),
                                f"{name}: tally key moved to {rows[0]['reason']!r}")

    def test_no_tally_key_carries_the_override_text(self):
        for name in DENIALS:
            with self.subTest(guard=name):
                sid = f"k-{name}"
                self.reason(name, sid=sid)
                for row in trace.read(sid):
                    self.assertNotIn("your own terminal", row.get("reason", ""))


class TestANewGuardCannotForgetIt(PersonaIso):
    """Structural, because remembering is what fails. Every denial goes through `_deny`,
    so the override rides along with a guard nobody has written yet."""

    @staticmethod
    def _source() -> str:
        return Path(hooks.__file__).read_text()

    @staticmethod
    def _emits_a_denial(fn: ast.FunctionDef) -> bool:
        """An `_emit(...)` call whose literal payload carries `permissionDecision: deny`.
        Deliberately not "the word `deny` appears": `_trace("deny", …)` records a denial
        that `_deny` already emitted, and flagging those would make this test noise."""
        for node in ast.walk(fn):
            if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                    and node.func.id == "_emit"):
                continue
            for d in ast.walk(node):
                if not isinstance(d, ast.Dict):
                    continue
                for k, v in zip(d.keys, d.values):
                    if (isinstance(k, ast.Constant) and k.value == "permissionDecision"
                            and isinstance(v, ast.Constant) and v.value == "deny"):
                        return True
        return False

    def test_deny_is_the_only_thing_that_emits_a_denial(self):
        tree = ast.parse(self._source())
        fns = [n for n in ast.walk(tree) if isinstance(n, ast.FunctionDef)]
        emitters = [f.name for f in fns if self._emits_a_denial(f)]
        self.assertEqual(["_deny"], emitters,
                         "a denial is emitted outside `_deny`, so it carries no override")

    def test_every_deny_call_passes_prose_and_not_a_prebuilt_message(self):
        """Precondition for the test above: the call sites really do exist and route here —
        every one of them, not "at least six", which is a number an eleventh guard never
        disturbed (#1000)."""
        calls = re.findall(r"_deny\(\s*\"PreToolUse\"", self._source())
        self.assertTrue(calls, "precondition: the guards were not found")
        self.assertEqual(len(_deny_call_sites()), len(calls),
                         "a `_deny` call that is not a PreToolUse denial with its own reason")


class TestTheDocumentationExists(unittest.TestCase):
    """"Undocumented override" and "no override" are the same thing to the person hitting
    one, so the denial's pointer must land somewhere real."""

    def setUp(self) -> None:
        self.text = DOC.read_text()

    def test_hooks_md_has_the_section(self):
        self.assertIn(SECTION, self.text)

    def test_the_section_rules_out_a_config_key_and_says_why(self):
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        self.assertIn("charter.toml", body)
        self.assertIn("committed", body)

    def test_the_section_names_the_move_that_works(self):
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        self.assertIn("terminal", body)

    def test_the_section_names_the_narrower_moves_that_come_first(self):
        """Some guards have a real, narrower answer. Sending someone to a terminal when
        `--apply` or an attended re-run is the actual fix would be a worse doc than none."""
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        self.assertIn("attended", body)
        self.assertIn("git-policy --apply", body)

    def test_the_narrower_moves_are_introduced_without_a_count(self):
        """#1000. "Six guards name a narrower move first" was a count nothing measures. No row
        and no denial carries whether a guard HAS a narrower move — every denial names a remedy,
        and which of those this section promotes is the page's own choice — so unlike the
        README's count of the guards there is no source to hold the number to. A number that
        cannot be held to anything lags the list under it, so the sentence states none."""
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        intro = next(p for p in body.split("\n\n") if "narrower move" in p)
        counts = "|".join([r"\d+", *_NUMBER_WORDS])
        self.assertNotRegex(intro, rf"(?i)\b({counts})\s+guards\b")

    def test_the_section_names_the_nuclear_option_as_not_an_override(self):
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        self.assertIn("uninstall", body)


if __name__ == "__main__":
    unittest.main()
