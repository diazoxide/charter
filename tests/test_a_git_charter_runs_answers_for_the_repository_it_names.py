"""#964 — a git charter runs answers for the repository its argv names, not the caller's.

git reads four variables that name a repository whatever the command line says:
``GIT_DIR``, ``GIT_WORK_TREE``, ``GIT_INDEX_FILE`` and ``GIT_COMMON_DIR``. ``-C <path>`` and
``cwd=`` only move where git starts, so with ``GIT_DIR=<repoA>/.git`` exported,
``git -C <repoB> branch --show-current`` answers repoA's branch — while
``rev-parse --show-toplevel`` still says repoB, so the call looks right. git exports
``GIT_DIR`` inside every hook it runs, which makes a charter started from one the ordinary
way in; an ``export`` left over in a shell is the other.

#942 gave `util.run` an ``unset=`` for this and one call site used it. Measured on main
before this change, with ``GIT_DIR`` pointing at repoA: `util.run`, `worktree.head_of`,
`freshness.head_sha`, `glstate`'s remote reads and the status line's `_run_state` all
answered for repoA. The last three never reached `util.run` at all.

Every repository here is a real one in a temp directory, and every expected answer is read
from it BEFORE the variable is exported — git's own answer, not one this file computes.
"""

from __future__ import annotations

import ast
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import charter
from charter import freshness, glstate, statusline, util, worktree

_CHARTER = Path(charter.__file__).resolve().parent


def _git(repo: Path, *args: str) -> str:
    """A fixture git, run with this process's environment as the suite left it."""
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True,
                          text=True).stdout.strip()


def _repo(path: Path, branch: str, tracked: str, origin: str) -> Path:
    subprocess.run(["git", "init", "-q", "-b", branch, str(path)], check=True,
                   capture_output=True)
    (path / tracked).write_text("x\n")
    _git(path, "add", tracked)
    _git(path, "commit", "-q", "-m", f"{branch} starts")
    _git(path, "remote", "add", "origin", origin)
    return path


def _same(a: str | Path, b: str | Path) -> bool:
    """One directory, however it was spelled: macOS hands out `/var/…` for `/private/var/…`,
    and git answers some of these relative to where it started."""
    return os.path.realpath(a) == os.path.realpath(b)


class TwoRepositories(unittest.TestCase):
    """repoA on `alpha`, holding `a.txt`, with `acme/alpha` as its origin; repoB on `bravo`,
    holding `b.txt`, with `acme/bravo`. Both clean. Every case asks about repoB."""

    def setUp(self) -> None:
        tmp = Path(tempfile.mkdtemp(prefix="charter-964-"))
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        self.a = _repo(tmp / "repoA", "alpha", "a.txt", "https://github.com/acme/alpha.git")
        self.b = _repo(tmp / "repoB", "bravo", "b.txt", "https://github.com/acme/bravo.git")

    def exported(self, **names: str):
        """This process's environment with *names* exported, as a git hook's would be."""
        return mock.patch.dict(os.environ, names)

    def a_hook_in_repo_a(self):
        return self.exported(GIT_DIR=str(self.a / ".git"))


class UtilRunAnswersForTheRepositoryItNames(TwoRepositories):

    def test_an_exported_git_dir_does_not_decide_which_branch_minus_c_reads(self):
        with self.a_hook_in_repo_a():
            said = util.run(["git", "-C", str(self.b), "branch", "--show-current"]).stdout
        self.assertEqual(said.strip(), "bravo")

    def test_an_exported_git_dir_does_not_decide_which_branch_cwd_reads(self):
        """`gitpolicy._git` and `planegit._git` name the repository with ``cwd=``, which git
        overrides exactly as it overrides ``-C``."""
        with self.a_hook_in_repo_a():
            said = util.run(["git", "branch", "--show-current"], cwd=self.b).stdout
        self.assertEqual(said.strip(), "bravo")

    def test_no_variable_that_names_a_repository_reaches_a_git_child(self):
        """Each of the four on its own, read back through the `rev-parse` question it moves."""
        b_git = self.b / ".git"
        cases = [
            ("GIT_DIR", str(self.a / ".git"), ["rev-parse", "--absolute-git-dir"], b_git),
            ("GIT_WORK_TREE", str(self.a), ["rev-parse", "--show-toplevel"], self.b),
            ("GIT_INDEX_FILE", str(self.a / ".git" / "index"),
             ["rev-parse", "--git-path", "index"], b_git / "index"),
            ("GIT_COMMON_DIR", str(self.a / ".git"), ["rev-parse", "--git-common-dir"], b_git),
        ]
        for name, value, question, answer in cases:
            with self.subTest(name), self.exported(**{name: value}):
                said = util.run(["git", "-C", str(self.b), *question]).stdout.strip()
                self.assertTrue(_same(self.b / said, answer),
                                f"with {name} exported git answered {said!r}, not {answer}")

    def test_a_config_handed_to_git_in_the_environment_still_reaches_it(self):
        """``GIT_CONFIG_COUNT`` is how CONTRIBUTING hands the suite the runner's git config, and
        ``GIT_CONFIG_GLOBAL`` is how `tests/_gitguard` keeps the operator's out. Neither names a
        repository, and withholding either would take away what the caller asked for."""
        with self.exported(GIT_CONFIG_COUNT="1", GIT_CONFIG_KEY_0="charter.probe",
                           GIT_CONFIG_VALUE_0="kept"):
            said = util.run(["git", "-C", str(self.b), "config", "--get", "charter.probe"],
                            check=False).stdout
        self.assertEqual(said.strip(), "kept")


class CharterGitReadsAnswerForTheRepositoryTheyName(TwoRepositories):
    """One public read per module that spawns git, each asked about repoB from inside a hook
    in repoA."""

    def test_worktree_reads_the_named_clones_branch(self):
        with self.a_hook_in_repo_a():
            self.assertEqual(worktree.head_of(self.b), ("bravo", False))

    def test_freshness_reads_the_named_repositorys_head(self):
        head = _git(self.b, "rev-parse", "HEAD")
        with self.a_hook_in_repo_a():
            self.assertEqual(freshness.head_sha(self.b), head)

    def test_the_forge_refresh_reads_the_named_clones_origin(self):
        with self.a_hook_in_repo_a():
            said = (glstate._remote_url(self.b), glstate._remote_path(self.b))
        self.assertEqual(said, ("https://github.com/acme/bravo.git", "acme/bravo"))

    def test_the_status_line_calls_a_clean_clone_clean(self):
        """Measured before the fix: repoA's index read against repoB's files — `a.txt` staged
        and missing, `b.txt` untracked — so a clean clone rendered dirty."""
        with self.a_hook_in_repo_a():
            state = statusline._run_state(self.b)
        self.assertEqual((state["dirty"], state["tracked_dirty"]), (False, False))


#: The `subprocess` entry points that take an argv.
_SPAWNS = frozenset({"run", "Popen", "call", "check_call", "check_output"})


def _bare_git_spawns(source: str) -> list[int]:
    """Line numbers of `subprocess.<spawn>(["git", …])` in *source*."""
    lines = []
    for node in ast.walk(ast.parse(source)):
        if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and node.func.attr in _SPAWNS and isinstance(node.func.value, ast.Name)
                and node.func.value.id == "subprocess"):
            continue
        argv = node.args[0] if node.args else next(
            (k.value for k in node.keywords if k.arg == "args"), None)
        if (isinstance(argv, (ast.List, ast.Tuple)) and argv.elts
                and isinstance(argv.elts[0], ast.Constant) and argv.elts[0].value == "git"):
            lines.append(node.lineno)
    return lines


class EveryGitCharterRunsGoesThroughUtilRun(unittest.TestCase):
    """The scrub lives in `util.run`, so a git spawned beside it has none. That is how five
    reads — two of them the status line's — kept answering for the caller's repository after
    #942 fixed one call.

    What this can see is the literal spelling, ``subprocess.run(["git", …])``: an argv built
    in a variable first walks past it. Every git charter spawned was spelled that way when
    this was written."""

    def test_the_reader_finds_a_bare_git_spawn(self):
        """So the empty answer below means none were found, not that none could be."""
        found = _bare_git_spawns(
            "import subprocess\n"
            "subprocess.run(['git', '-C', d, 'status'], timeout=3)\n"
            "subprocess.Popen(args=('git', 'fetch'))\n"
            "subprocess.run(['gh', 'api'])\n")
        self.assertEqual(found, [2, 3])

    def test_no_module_spawns_git_around_util_run(self):
        offenders = [f"{path.relative_to(_CHARTER.parent)}:{line}"
                     for path in sorted(_CHARTER.rglob("*.py"))
                     for line in _bare_git_spawns(path.read_text(encoding="utf-8"))]
        self.assertEqual(offenders, [], "spawn git through `util.run`, which withholds the "
                                        "variables that would point it at another repository")


if __name__ == "__main__":
    unittest.main()
