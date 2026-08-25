"""memory-sync: commits pending persona memory, refuses on a secret. Uses a real tmp
git repo (config.ROOT is redirected to it)."""
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from charter import config, persona, commands_persona
from tests import _envguard
from tests._isolation import PersonaIso

_KEYS = ("ROOT", "PERSONAS_DIR", "STATE_DIR", "PERSONA_STATE_DIR", "ACTIVE_PERSONA_FILE")


def _git(root, *a):
    return subprocess.run(["git", "-C", str(root), *a], stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL, text=True)


class _Args:
    def __init__(self, no_push=True):
        self.no_push = no_push


class TestMemorySync(unittest.TestCase):
    def setUp(self):
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        self.tmp = Path(tempfile.mkdtemp(prefix="edm-memsync-"))
        _git(self.tmp, "init", "-q")
        _git(self.tmp, "config", "user.email", "t@t")
        _git(self.tmp, "config", "user.name", "t")
        _git(self.tmp, "config", "commit.gpgsign", "false")
        self._orig = {k: getattr(config, k) for k in _KEYS}
        config.ROOT = self.tmp
        config.PERSONAS_DIR = self.tmp / "personas"
        config.STATE_DIR = self.tmp / ".charter"
        config.PERSONA_STATE_DIR = config.STATE_DIR / "persona-state"
        config.ACTIVE_PERSONA_FILE = config.STATE_DIR / "active-persona"
        d = config.PERSONAS_DIR / "qa"
        d.mkdir(parents=True)
        (d / "persona.md").write_text("---\nname: qa\nrole: QA\nvault: qa\n---\n\n# QA\n")
        persona.scaffold_memory("qa")
        _git(self.tmp, "add", "-A")
        _git(self.tmp, "commit", "-q", "-m", "init")
        self.addCleanup(self._restore)

    def _restore(self):
        for k, v in self._orig.items():
            setattr(config, k, v)
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _pending(self):
        return _git(self.tmp, "status", "--porcelain", "--", "personas").stdout.strip()

    def test_nothing_to_sync(self):
        self.assertEqual(commands_persona.cmd_persona_memory_sync(_Args()), 0)

    def test_commits_pending_memory(self):
        persona.remember("qa", "a durable fact about routing coverage", title="routing fact")
        self.assertNotEqual(self._pending(), "")  # dirty before
        self.assertEqual(commands_persona.cmd_persona_memory_sync(_Args(no_push=True)), 0)
        self.assertEqual(self._pending(), "")      # clean after

    def test_refuses_on_secret(self):
        persona.remember("qa", "the api_key = sk-abcdef123456 lives here", title="leak")
        self.assertEqual(commands_persona.cmd_persona_memory_sync(_Args(no_push=True)), 1)
        self.assertIn("leak", self._pending())     # left uncommitted


if __name__ == "__main__":
    unittest.main()


class EveryPlaneCommitOffersTheForgeToken(PersonaIso):
    """charter's headline rule is one credential: each repo's own forge's token over
    HTTPS, never SSH. `commands.commit_push` obeyed it; `cmd_persona_memory_sync` had
    grown a second committer that ran `git push origin HEAD` — over SSH — on the ONE
    memory path the SessionStart hook explicitly tells an agent to use. It reported
    "Committed locally, but push failed (check git auth)" while `gh auth status` was fine,
    because the token was never offered.

    `tests/test_persona_memory_sync.py` only ever passed `no_push=True`, so the push argv
    — the only place the difference was visible — was never exercised. This asserts the
    argv, which is what the two implementations disagreed about.

    `PersonaIso`, not `unittest.TestCase`, and not for tidiness: every git call here is
    faked, which made the class read as pure argv inspection — but `commit_push` is REAL,
    and its last act on the pushed path is `planegit.record_push`, which DELETES
    `<STATE_DIR>/plane-push.json`. Unisolated, that is the developer's own record of a
    stranded push, the one `doctor` reads to tell them their plane never reached the
    forge; the suite erased it on every run and `record_push`'s own `except OSError: pass`
    guaranteed silence. Found by `tests/_planeguard.py` rather than by hand, which is what
    that guard is for.
    """

    def test_memory_sync_delegates_to_the_one_committer(self):
        from unittest import mock
        from charter import commands_persona, planegit
        with mock.patch.object(commands_persona, "_pending_memory",
                               return_value=["personas/p/memory/m.md"]), \
             mock.patch.object(planegit, "commit_push", return_value=0) as cp, \
             mock.patch("charter.hooks._secret_kind", return_value=None):
            rc = commands_persona.cmd_persona_memory_sync(_Args(no_push=False))
        self.assertEqual(rc, 0)
        cp.assert_called_once()
        self.assertEqual(cp.call_args.args[1][:2], ["add", "--"])

    def test_every_push_offers_the_forge_token(self):
        """The invariant, asserted on the ARGV rather than by grepping for the word.

        A grep for `"push"` outside planegit flags `SHARE_MODES = (…, "push")` and every
        `no_push=` kwarg, and would still miss a new `[*flags, "push", …]`. What actually
        matters is that whatever git is asked to do carries `-c credential.helper=<forge>`
        — that is the whole of charter's one-credential rule at the point of use.
        """
        import subprocess
        from unittest import mock
        from charter import planegit

        seen = []

        diffs = []

        def fake_run(cmd, **kw):
            seen.append(list(cmd))
            rc = 0
            if "diff" in cmd and "--quiet" in cmd:
                # `commit_push` asks twice: once before committing (non-zero = something
                # staged, so proceed) and once after (non-zero would mean the commit
                # failed). A stub that answers the same both times never reaches the push.
                diffs.append(1)
                rc = 1 if len(diffs) == 1 else 0
            return subprocess.CompletedProcess(cmd, rc, stdout="main\n", stderr="")

        class _Forge:
            cli = "gh"
            def credential_helper(self):
                return "!gh auth git-credential"

        from charter import gitpolicy
        with mock.patch.object(planegit.util, "run", side_effect=fake_run), \
             mock.patch.object(gitpolicy, "forge_for", return_value=_Forge()), \
             mock.patch.object(planegit, "_origin_https",
                               return_value="https://github.com/demo/p.git"):
            planegit.commit_push(Path("/tmp/x"), ["add", "-A"], "m")

        pushes = [c for c in seen if "push" in c]
        self.assertTrue(pushes, "commit_push never pushed — the assertion would be vacuous")
        for argv in pushes:
            self.assertIn("credential.helper", " ".join(argv),
                          f"a push without the forge token: {argv}")
            self.assertFalse([a for a in argv if a.startswith("git@")],
                             f"an SSH remote reached a push: {argv}")
