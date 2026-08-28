"""#504: `core.worktree` is the third spelling of the work tree, and it is in no argv.

`hooks._git_target` answers with every directory a git invocation's global options and
environment aim it at — the cwd, `-C`, `--work-tree`/`GIT_WORK_TREE`, `--git-dir`/`GIT_DIR`.
Git has one more, and it is not a token at all: the `core.worktree` key in a repository's
own `.git/config`. A repository carrying it has the named directory as its working tree for
**every** command it runs, so a guard reading argv and environment sees a plain
`git checkout feature` typed inside a workspace clone.

Reproduced end to end on git 2.50.1 before a line was written, and the plane's file changed:

    git clone <plane> /tmp/cfgclone
    git -C /tmp/cfgclone config core.worktree <plane>
    git -C /tmp/cfgclone rev-parse --show-toplevel     # -> <plane>
    git -C /tmp/cfgclone checkout feature              # -> <plane>/f.txt is now the branch's

**The oracle here is git itself.** Charter reads the config rather than running git — a
subprocess is ten milliseconds on the PreToolUse path, where the common case exits on a
string comparison — so the thing that could go wrong is charter's reader disagreeing with
git's. Every value form below is therefore checked twice: what `charter.gitconfig` says, and
what `git rev-parse --show-toplevel` says about the same file. A reader that agrees with git
on the forms git writes is the claim; a reader that agrees with an expectation somebody
typed is a copy of that person's belief.

`tests/test_plane_root_checkout_is_two_commands.py` carries the same route through the real
hook, crossed with every command in its corpus. This module is about the reader underneath.
"""

from __future__ import annotations

import subprocess
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from charter import gitconfig


def _git(*args, cwd=None):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)


class GitConfigCase(unittest.TestCase):
    """A repository, a directory to point it at, and a way to rewrite one key by hand.

    The key is written into the file directly rather than with `git config`, because a
    repository whose `core.worktree` names a directory that does not exist is one git
    refuses to run in at all — including refusing the `git config` that would fix it. Every
    value form this module is about has to be reachable, and half of them are not reachable
    through git's own writer once the first one is in place.
    """

    def setUp(self):
        # Resolved: a macOS temp directory lives under /var/folders, itself a link to
        # /private/var, and every answer below comes back resolved.
        self.tmp = Path(tempfile.mkdtemp(prefix="charter-gitconfig-")).resolve()
        self.addCleanup(self._clean)
        self.repo = self.tmp / "repo"
        self.elsewhere = self.tmp / "elsewhere"
        self.spaced = self.tmp / "pl ane"
        for d in (self.repo, self.elsewhere, self.spaced):
            d.mkdir(parents=True)
        _git("init", "-q", "-b", "main", str(self.repo))
        self.config = self.repo / ".git" / "config"

    def _clean(self):
        import shutil
        shutil.rmtree(self.tmp, ignore_errors=True)

    def write(self, body: str) -> None:
        """Replace the repository config with *body*, keeping what git needs to run."""
        self.config.write_text("[core]\n\trepositoryformatversion = 0\n\tbare = false\n"
                               + body)

    def git_top(self):
        """git's own answer, or ``None`` when git refuses the repository outright."""
        got = _git("-C", str(self.repo), "rev-parse", "--show-toplevel")
        return Path(got.stdout.strip()).resolve() if got.returncode == 0 else None

    def mine(self, cwd=None):
        got = gitconfig.configured_work_tree(cwd or self.repo)
        return Path(got).resolve() if got is not None else None


class CharterReadsWhatGitReads(GitConfigCase):
    """Every value form git writes, checked against git's own answer.

    The forms are not invented: absolute is what `git config core.worktree <path>` writes,
    relative is what a hand-edited config carries, and the quoted/spaced/commented ones are
    what git's own writer produces for a value that needs them.
    """

    def test_each_form_of_the_value_agrees_with_git(self):
        forms = {
            "absolute": lambda: str(self.elsewhere),
            "relative to the GIT DIR": lambda: "../../elsewhere",
            "quoted, with a space": lambda: f'"{self.spaced}"',
            "unquoted, with a space": lambda: str(self.spaced),
            "with a trailing comment": lambda: f"{self.elsewhere} # why",
            "with trailing whitespace": lambda: f"{self.elsewhere}\t ",
            "the section header in capitals": lambda: str(self.elsewhere),
        }
        for label, make in forms.items():
            with self.subTest(form=label):
                head = "[CORE]" if "capitals" in label else "[core]"
                self.write(f"{head}\n\tworktree = {make()}\n")
                theirs = self.git_top()
                self.assertIsNotNone(theirs, f"git refused the fixture for {label}")
                self.assertEqual(self.mine(), theirs)

    def test_a_relative_value_resolves_against_the_git_dir_and_not_the_work_tree(self):
        """Stated on its own because the wrong base looks right. `../elsewhere` is what you
        write if you resolve against the repository's directory, and git refuses the
        repository outright for it — the value it honours is `../../elsewhere`, one level
        further up, because the base is `<repo>/.git`."""
        self.write(f"[core]\n\tworktree = ../elsewhere\n")
        self.assertIsNone(self.git_top(), "git now accepts the work-tree-relative form")
        self.write(f"[core]\n\tworktree = ../../elsewhere\n")
        self.assertEqual(self.mine(), self.elsewhere)
        self.assertEqual(self.mine(), self.git_top())

    def test_a_repository_with_no_such_key_names_nothing(self):
        self.assertIsNone(gitconfig.configured_work_tree(self.repo))

    def test_a_subsection_is_a_different_key(self):
        """`[core "x"] worktree` is `core.x.worktree`, which relocates nothing — and reads
        exactly like `[core]` to a scanner that takes the first word of the header."""
        self.write(f'[core "x"]\n\tworktree = {self.elsewhere}\n')
        self.assertEqual(self.git_top(), self.repo.resolve())
        self.assertIsNone(self.mine())

    def test_the_last_value_wins_the_way_it_does_in_git(self):
        other = self.tmp / "second"
        other.mkdir()
        self.write(f"[core]\n\tworktree = {self.elsewhere}\n[core]\n\tworktree = {other}\n")
        self.assertEqual(self.mine(), other.resolve())
        self.assertEqual(self.mine(), self.git_top())

    def test_a_commented_out_key_is_not_a_key(self):
        for line in (f"#\tworktree = {self.elsewhere}", f";\tworktree = {self.elsewhere}"):
            with self.subTest(comment=line[0]):
                self.write(f"[core]\n{line}\n")
                self.assertEqual(self.git_top(), self.repo.resolve())
                self.assertIsNone(self.mine())

    def test_a_value_continued_onto_the_next_line(self):
        """A trailing backslash continues a value in git's config format. Rare for a path
        and cheap to read correctly; the alternative is a truncated directory name, which
        would add a subject naming somewhere nobody asked about."""
        head, tail = str(self.elsewhere)[:6], str(self.elsewhere)[6:]
        self.write(f"[core]\n\tworktree = {head}\\\n{tail}\n")
        self.assertEqual(self.mine(), self.git_top())
        self.assertEqual(self.mine(), self.elsewhere)


class WhichRepositoryItAsks(GitConfigCase):
    """Discovery, done the way git does it and without running git."""

    def test_from_a_subdirectory_of_the_repository(self):
        self.write(f"[core]\n\tworktree = {self.elsewhere}\n")
        deep = self.repo / "a" / "b"
        deep.mkdir(parents=True)
        self.assertEqual(self.mine(cwd=deep), self.elsewhere)

    def test_a_named_git_dir_skips_the_walk_entirely(self):
        """With `--git-dir`/`GIT_DIR` there is nothing to discover, and the cwd is not the
        repository — which is #477's whole shape one option over."""
        self.write(f"[core]\n\tworktree = {self.elsewhere}\n")
        got = gitconfig.configured_work_tree("/nowhere/at/all", self.repo / ".git")
        self.assertEqual(Path(got).resolve(), self.elsewhere.resolve())

    def test_a_dot_git_FILE_is_followed_one_hop(self):
        """A submodule's `.git` is a file naming the real git dir under the superproject.
        Built by hand rather than with `git submodule add`: the layout is the thing under
        test, and a real submodule costs a clone to assert the same two lines.
        """
        modules = self.repo / ".git" / "modules" / "sub"
        modules.mkdir(parents=True)
        (modules / "config").write_text(f"[core]\n\tworktree = {self.elsewhere}\n")
        sub = self.repo / "sub"
        sub.mkdir()
        (sub / ".git").write_text("gitdir: ../.git/modules/sub\n")
        self.assertEqual(self.mine(cwd=sub), self.elsewhere)

    def test_a_pointer_that_names_nothing_is_not_followed(self):
        sub = self.repo / "sub"
        sub.mkdir()
        for junk in ("", "not a pointer\n", "gitdir:\n", "gitdir\n"):
            with self.subTest(pointer=repr(junk)):
                (sub / ".git").write_text(junk)
                self.assertIsNone(gitconfig.configured_work_tree(sub))

    def test_no_repository_above_the_directory_names_nothing(self):
        self.assertIsNone(gitconfig.configured_work_tree(self.tmp))


class WhatItCostsAndWhatItRefusesToCost(GitConfigCase):
    """The reason #497 filed this instead of doing it: a config read on the hot path."""

    def test_it_does_not_run_git(self):
        """The property, asserted rather than timed. `git rev-parse --show-toplevel`
        answers this question exactly and costs a process — some ten milliseconds inside a
        PreToolUse hook that runs on every Bash call. A later simplification onto it would
        be invisible to a wall-clock ceiling on a fast machine and is the one regression
        worth naming, so every way of starting a process raises here.
        """
        self.write(f"[core]\n\tworktree = {self.elsewhere}\n")

        def refuse(*a, **kw):
            raise AssertionError("charter ran a process to read a config key")

        with mock.patch.object(subprocess, "run", refuse), \
                mock.patch.object(subprocess, "Popen", refuse):
            self.assertEqual(self.mine(), self.elsewhere)

    def test_and_it_stays_in_the_microseconds(self):
        """A ceiling with two orders of magnitude of headroom, which is what makes it a
        regression detector rather than a flaky timer. Measured on the machine this was
        written on: 13 µs with no repository above the cwd, 35 µs at a repository root
        with no such key, 47 µs two directories down inside one, and 65 µs with the key
        present. The ceiling is 2 ms, so a loaded CI runner has room and a subprocess does
        not.
        """
        self.write(f"[core]\n\tworktree = {self.elsewhere}\n")
        start = time.perf_counter()
        for _ in range(100):
            gitconfig.configured_work_tree(self.repo)
        each = (time.perf_counter() - start) / 100
        self.assertLess(each, 0.002, f"{each * 1e6:.0f} µs per call")

    def test_a_config_larger_than_the_bound_is_not_read_past_it(self):
        """The bound, and the direction it fails in, said out loud. A key past
        `MAX_CONFIG_BYTES` is not found, which answers "no work tree named" — so the guard
        is exactly as strong as it was before this existed, rather than hanging on a file
        that is not a config.
        """
        padding = "\t; " + "x" * 200 + "\n"
        body = ("[core]\n" + padding * (gitconfig.MAX_CONFIG_BYTES // len(padding) + 8)
                + f"\tworktree = {self.elsewhere}\n")
        self.write(body)
        self.assertGreater(self.config.stat().st_size, gitconfig.MAX_CONFIG_BYTES)
        self.assertEqual(self.git_top(), self.elsewhere.resolve(),
                         "the fixture no longer reaches git, so the bound proves nothing")
        self.assertIsNone(self.mine())

    def test_nothing_here_raises(self):
        """Same promise the rest of the guard path keeps: a hook may cost a session its
        briefing and never its turn."""
        for bad in ("", "\x00/x", "/nonexistent/deep/path", self.tmp / "gone"):
            with self.subTest(cwd=repr(bad)):
                self.assertIsNone(gitconfig.configured_work_tree(bad))
        self.config.write_bytes(b"[core]\n\tworktree = \xff\xfe\n")
        self.assertIsNotNone(gitconfig.configured_work_tree(self.repo))


if __name__ == "__main__":
    unittest.main()
