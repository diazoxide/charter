"""The secret guards report the KIND, never the value — pinned at the source.

Three code-scanning alerts (CodeQL `py/clear-text-logging-sensitive-data`, alerts #2 and
#3) traced five flows from `hooks._secret_kind` to two printers: `hooks._emit`
(`print(json.dumps(obj))`, which the harness reads and a transcript can keep) and
`util.err` (`print(…, file=sys.stderr)`). Every one of them is a false positive, and they
are all false for the SAME reason: `_secret_kind` answers with a label out of the
`_SECRET_CHECKS` tuple — "AWS access key", "JWT" — and never with anything taken from the
text it scanned. CodeQL cannot see that, because its heuristic classifies a value by the
NAME of the function that produced it, and this one is called `_secret_kind`.

The dismissal on those alerts points here, so the claim it rests on has to be checkable
and has to stay true. What makes a guard like this dangerous is exactly the change that
looks like an improvement — putting the offending line in the message so the author can
find it — so the property is pinned where it would break, at the source, and again at
each of the three real call sites that print the answer:

  * `hooks._posttooluse_secret_scan`  → `_emit`, to the harness (alert #2)
  * `planegit.commit_push`            → `util.err`, `charter save` (alert #3, flow 3)
  * `commands_persona.cmd_persona_memory_sync` → `util.err` (alert #3, flows 0-2)

The one place charter DOES print a secret value is `cmd_secret_reveal`, which is a
different thing entirely: gated behind `--force`, traced before the write, and preceded by
a warning. Nothing here is about that path.
"""

from __future__ import annotations

import io
import json
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from charter import commands_persona, config, hooks, persona, planegit
from charter.hooks import _SECRET_CHECKS, _secret_kind
from tests._isolation import PersonaIso

#: Two DIFFERENT live specimens per rule in `_SECRET_CHECKS`. Two, because one specimen
#: only proves the answer is a label *this time*; a pair of them proves the answer does
#: not vary with the input at all — which is the actual property, and the one a
#: `f"{label}: {m.group(0)}"` "helpful" rewrite breaks.
_SPECIMENS: tuple[tuple[str, str, str], ...] = (
    ("AgentMail key",
     "am_us_9f2ac41b8e0d4c7a",
     "am_us_TTTTvvvv1111wwww"),
    ("JWT",
     "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhbGljZSJ9",
     "eyJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJib2IifQ"),
    ("private key (PEM)",
     "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA9x2\n",
     "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEA\n"),
    ("AWS access key",
     "AKIAQ7VZ3RJHT2LMNPQR",
     "AKIA5555XXXX8888YYYY"),
    ("credential assignment",
     'api_key = "sk-live-8Hq2Wm4Tz9Rb"',
     "password: hunter2-not-a-real-one"),
)

_LABELS = tuple(label for label, _rx in _SECRET_CHECKS)


class TestEverySpecimenIsActuallyDetected(unittest.TestCase):
    """The fixture's own load-bearing assumption. A specimen that quietly stops matching
    turns every assertion below into a statement about `None`, which is the failure mode
    this repository keeps finding: a test that passes because it can no longer see the
    thing it measures."""

    def test_each_rule_has_two_specimens_that_hit_it(self):
        self.assertEqual(len(_SPECIMENS), len(_SECRET_CHECKS),
                         "a rule was added to _SECRET_CHECKS with no specimen here")
        for expected, a, b in _SPECIMENS:
            with self.subTest(expected):
                self.assertEqual(_secret_kind(a), expected)
                self.assertEqual(_secret_kind(b), expected)
        self.assertIsNone(_secret_kind("an ordinary memo about routing coverage"))


class TestTheKindIsALabelNotTheMatch(unittest.TestCase):
    """The source property, and the whole basis of the dismissal on alerts #2 and #3."""

    def test_the_answer_is_one_of_the_declared_labels(self):
        for expected, a, b in _SPECIMENS:
            for text in (a, b):
                with self.subTest(expected, text=text[:12]):
                    self.assertIn(_secret_kind(text), _LABELS)

    def test_two_different_secrets_of_one_kind_give_the_identical_answer(self):
        """The answer is a function of the RULE that matched, not of the text. This is
        what fails the moment anyone splices the offending value into the label."""
        for expected, a, b in _SPECIMENS:
            with self.subTest(expected):
                self.assertNotEqual(a, b)
                self.assertEqual(_secret_kind(a), _secret_kind(b))

    def test_no_part_of_the_secret_survives_into_the_answer(self):
        for expected, a, b in _SPECIMENS:
            for text in (a, b):
                kind = _secret_kind(text)
                for chunk in text.split():
                    if len(chunk) < 6:
                        continue
                    with self.subTest(expected, chunk=chunk[:16]):
                        self.assertNotIn(chunk, kind)

    def test_surrounding_text_does_not_reach_the_answer(self):
        """The scanned text is a whole memory file in the field, not a bare token."""
        kind = _secret_kind(
            "# Deploy notes\n\nThe prod runner uses AKIAQ7VZ3RJHT2LMNPQR for S3.\n")
        self.assertEqual(kind, "AWS access key")
        self.assertNotIn("AKIA", kind)
        self.assertNotIn("prod runner", kind)


class TestTheHookWarningCarriesNoValue(PersonaIso):
    """Alert #2: `_emit` prints JSON the harness reads, so anything in that object can
    land in a transcript."""

    def _scan(self, body: str, name: str = "leak.md") -> str:
        d = Path(config.ROOT) / "personas" / "qa" / "memory"
        d.mkdir(parents=True, exist_ok=True)
        fp = d / name
        fp.write_text(body)
        out = io.StringIO()
        with redirect_stdout(out):
            hooks._posttooluse_secret_scan({"content": body}, str(fp), "sess-1")
        return out.getvalue()

    def test_it_warns_at_all(self):
        emitted = self._scan("aws key AKIAQ7VZ3RJHT2LMNPQR here\n")
        self.assertIn("SECURITY", emitted)
        self.assertIn("AWS access key", emitted)
        self.assertIn("leak.md", emitted)

    def test_the_emitted_object_holds_no_specimen(self):
        for expected, a, b in _SPECIMENS:
            for text in (a, b):
                with self.subTest(expected, text=text[:12]):
                    emitted = self._scan(f"a memo\n\n{text}\n")
                    self.assertIn(expected, emitted)
                    for chunk in text.split():
                        if len(chunk) >= 6:
                            self.assertNotIn(chunk, emitted)

    def test_what_it_emits_is_one_json_object_naming_the_kind(self):
        emitted = self._scan('token = "gh-p-Zq81LmVw03Rk"\n')
        obj = json.loads(emitted)
        blob = json.dumps(obj)
        self.assertIn("credential assignment", blob)
        self.assertNotIn("gh-p-Zq81LmVw03Rk", blob)

    def test_a_clean_memory_file_emits_nothing(self):
        self.assertEqual(self._scan("a durable fact about routing coverage\n"), "")


def _git(root, *a):
    return subprocess.run(["git", "-C", str(root), *a], stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL, text=True)


class TestTheSaveRefusalCarriesNoValue(PersonaIso):
    """Alert #3, flow 3: `charter save`'s own guard, which prints through `util.err`."""

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.root = Path(config.ROOT)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.root)],
                       check=True, capture_output=True)
        _git(self.root, "config", "user.email", "t@e")
        _git(self.root, "config", "user.name", "t")
        (self.root / "seed").write_text("x\n")
        _git(self.root, "add", "-A")
        _git(self.root, "-c", "commit.gpgsign=false", "commit", "-qm", "seed")
        _git(self.root, "remote", "add", "origin", "https://github.com/acme/plane.git")

    def _save_with(self, body: str) -> tuple[int, str]:
        d = self.root / "personas" / "qa" / "memory"
        d.mkdir(parents=True, exist_ok=True)
        (d / "leak.md").write_text(body)
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            rc = planegit.commit_push(self.root, ["add", "personas"], "m", no_push=True)
        return rc, err.getvalue()

    def test_it_refuses_and_names_the_kind_and_the_file(self):
        rc, err = self._save_with("aws key AKIAQ7VZ3RJHT2LMNPQR here\n")
        self.assertEqual(rc, 1)
        self.assertIn("AWS access key", err)
        self.assertIn("personas/qa/memory/leak.md", err)

    def test_the_refusal_holds_no_specimen(self):
        for expected, a, b in _SPECIMENS:
            for text in (a, b):
                with self.subTest(expected, text=text[:12]):
                    rc, err = self._save_with(f"a memo\n\n{text}\n")
                    self.assertEqual(rc, 1)
                    self.assertIn(expected, err)
                    for chunk in text.split():
                        if len(chunk) >= 6:
                            self.assertNotIn(chunk, err)


class TestTheMemorySyncRefusalCarriesNoValue(PersonaIso):
    """Alert #3, flows 0-2: the same refusal on `charter persona memory-sync`, which is
    the path the SessionStart hook actually tells an agent to use.

    `PersonaIso` rather than a hand-rolled `config.use` + restore. This module sorts near
    the front of discovery, so a restore that re-derives instead of putting back exactly
    what it took would hand the rest of the run a config nobody wrote — the failure the
    base class's own docstring is mostly about."""

    def setUp(self) -> None:
        super().setUp()
        root = Path(config.ROOT)
        _git(root, "init", "-q")
        _git(root, "config", "user.email", "t@t")
        _git(root, "config", "user.name", "t")
        _git(root, "config", "commit.gpgsign", "false")
        d = config.PERSONAS_DIR / "qa"
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text("---\nname: qa\nrole: QA\nvault: qa\n---\n\n# QA\n")
        persona.scaffold_memory("qa")
        _git(root, "add", "personas")
        _git(root, "commit", "-q", "-m", "init")

    class _Args:
        no_push = True

    def _sync_with(self, body: str) -> tuple[int, str]:
        persona.remember("qa", body, title="leak")
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            rc = commands_persona.cmd_persona_memory_sync(self._Args())
        return rc, err.getvalue()

    def test_it_refuses_and_names_the_kind(self):
        rc, err = self._sync_with("aws key AKIAQ7VZ3RJHT2LMNPQR here")
        self.assertEqual(rc, 1)
        self.assertIn("AWS access key", err)

    def test_the_refusal_holds_no_specimen(self):
        rc, err = self._sync_with('the api_key = "sk-live-8Hq2Wm4Tz9Rb" lives here')
        self.assertEqual(rc, 1)
        self.assertIn("credential assignment", err)
        self.assertNotIn("sk-live-8Hq2Wm4Tz9Rb", err)


if __name__ == "__main__":
    unittest.main()
