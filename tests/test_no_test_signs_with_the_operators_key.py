"""The seventh tripwire: `git` reads the repository's configuration, never the operator's.

`test_no_test_reads_the_operators_shell` covers what the launching shell exports;
`test_no_test_reads_the_operators_terminal` covers what its file descriptors are. This
covers the one file charter never writes and every `git` it spawns reads — ``~/.gitconfig``
— and the reason it is loud rather than merely redirected is that its wrong answer is not a
wrong answer at all. It is a **hang**.

A fixture repository inherits ``commit.gpgsign = true`` from the machine's own config. With
1Password's ``op-ssh-sign`` as ``gpg.ssh.program``, `git commit` parks on a biometric
prompt (#641). Measured on the machine this was written on, in a bare temp repository::

    $ git init -q . && echo x > a && git add a && git commit -m probe
    error: 1Password: failed to fill whole buffer
    fatal: failed to write commit object
    git commit -m probe  0.01s user 0.01s system 0% cpu 1:00.36 total

Sixty seconds and a failed commit with stdin closed; indefinite with a terminal attached.
No pass, no fail, no verdict. **A runner has no signing config**, so CI cannot see this and
never will — it costs developers, on their own machines, in the shape hardest to diagnose.

`WhatIsAnswered` pins the redirect, and `TheThirtySecondModule` is the case #641 is about,
written on purpose: a fixture repository, a plain `git commit`, and no neutraliser of its
own anywhere. Before `tests/_gitguard.py` that case was the hang; after it, it is a test.
"""

from __future__ import annotations

import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest

from tests import _gitguard, _planeguard


def _git(where, *args, env=None):
    return subprocess.run(["git", "-C", str(where), *args], text=True,
                          capture_output=True, env=env)


class WhatIsAnswered(unittest.TestCase):
    """Move one: every git child reads a file this repository wrote."""

    def test_the_three_variables_point_a_child_away_from_the_machine(self):
        self.assertEqual(os.environ["GIT_CONFIG_GLOBAL"], _gitguard.FILE)
        self.assertEqual(os.environ["GIT_CONFIG_SYSTEM"], os.devnull)
        self.assertEqual(os.environ["GIT_CONFIG_NOSYSTEM"], "1")

    def test_a_child_really_reads_it_rather_than_the_operators(self):
        """Asked of `git` itself, not of the environment: the variables being set proves
        the intent, and this proves the effect. The address is one only this suite's file
        carries, so the assertion cannot pass by coincidence on somebody's machine."""
        proc = subprocess.run(["git", "config", "--get", "user.email"],
                              text=True, capture_output=True)
        self.assertEqual(proc.stdout.strip(), "suite@charter.invalid")

    def test_every_key_the_config_states_is_the_one_git_answers_with(self):
        """Asked of `git`, key by key, and every value spelled out here rather than read
        back off `_gitguard.CONFIG` — a case that compared the file against the constant it
        was written from would pass whatever either of them said."""
        for key, value in (("user.name", "charter test suite"),
                           ("user.email", "suite@charter.invalid"),
                           ("commit.gpgsign", "false"), ("tag.gpgsign", "false"),
                           ("init.defaultBranch", "main"), ("core.hooksPath", "")):
            with self.subTest(key=key):
                proc = subprocess.run(["git", "config", "--get", key],
                                      text=True, capture_output=True)
                self.assertEqual(proc.returncode, 0, f"{key} is not set at all")
                self.assertEqual(proc.stdout.strip(), value)

    def test_the_file_git_reads_is_the_one_this_package_wrote(self):
        self.assertEqual(pathlib.Path(_gitguard.FILE).read_text(), _gitguard.CONFIG)
        self.assertTrue(_gitguard.FILE.startswith(tempfile.gettempdir()),
                        "the suite's config must not be a path on this developer's own "
                        f"machine: {_gitguard.FILE}")

    def test_installing_twice_does_not_hand_out_a_second_config_file(self):
        """Reachable twice — `tests` can be imported by a child process that also imports a
        test module — and a second install would repoint every later git child at a file
        this run had not written yet."""
        before = (_gitguard.FILE, os.environ["GIT_CONFIG_GLOBAL"])
        _gitguard.install()
        self.assertEqual((_gitguard.FILE, os.environ["GIT_CONFIG_GLOBAL"]), before)

    def test_the_redirect_is_installed_before_charter_is_imported(self):
        """Ordering, read off the source because that is where it lives. `charter.config`
        resolves a plane at import and reads a git worktree pointer on the way, so a
        redirect installed below that import is a redirect that arrives one plane too late.
        A comment saying so is not a test."""
        src = (pathlib.Path(__file__).parent / "__init__.py").read_text()
        self.assertLess(src.index("_gitguard.install()"), src.index("import _ttyguard"))


class TheThirtySecondModule(unittest.TestCase):
    """Move two: the case #641 is about, written deliberately.

    A fixture repository, a plain `git commit`, and no neutraliser anywhere in this module —
    no ``-c commit.gpgsign=false`` on the argv, no ``git config`` in `setUp`, no hand-built
    environment. That is the shape thirty-one modules here were each protecting themselves
    from by hand, in three different spellings, and the shape the thirty-second was always
    going to forget.
    """

    def setUp(self):
        self.repo = pathlib.Path(tempfile.mkdtemp(prefix="charter-signing-"))
        self.addCleanup(shutil.rmtree, self.repo, True)
        self.assertEqual(_git(self.repo, "init", "-q", ".").returncode, 0)
        (self.repo / "a").write_text("x\n")
        self.assertEqual(_git(self.repo, "add", "a").returncode, 0)

    def test_a_commit_in_a_fixture_repo_completes(self):
        proc = _git(self.repo, "commit", "-qm", "probe")
        self.assertEqual(proc.returncode, 0, proc.stderr)

    def test_and_it_is_not_signed(self):
        """``%G?`` is git's own verdict on a commit's signature, and ``N`` is "no
        signature". On a machine with no signing config this passes with the redirect
        deleted — and on the machine that filed #641 it is the difference between a test and
        a biometric prompt. That asymmetry is the whole reason the guard below exists: the
        environment that can catch this is never CI's."""
        self.assertEqual(_git(self.repo, "commit", "-qm", "probe").returncode, 0)
        self.assertEqual(_git(self.repo, "log", "-1", "--format=%G?").stdout.strip(), "N")

    def test_and_it_is_committed_by_the_suite_rather_than_by_whoever_ran_it(self):
        self.assertEqual(_git(self.repo, "commit", "-qm", "probe").returncode, 0)
        self.assertEqual(_git(self.repo, "log", "-1", "--format=%an <%ae>").stdout.strip(),
                         "charter test suite <suite@charter.invalid>")

    def test_and_its_branch_is_main_on_every_machine(self):
        self.assertEqual(_git(self.repo, "commit", "-qm", "probe").returncode, 0)
        self.assertEqual(_git(self.repo, "branch", "--show-current").stdout.strip(), "main")


class TheGuardIsNotBlind(unittest.TestCase):
    """Move three: a git child that steps outside the redirect is refused, for real.

    The redirect alone would only be a default, and #641 is a story about a default nobody
    noticed they had walked past. Same control as the other spawn tripwires: the refusal is
    made to happen, and a marker file that stays absent is the evidence that nothing ran.
    """

    def setUp(self):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="gitconfigguard-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.marker = self.dir / "the-cli-ran"
        for name in ("git", "tar"):
            p = self.dir / name
            p.write_text(f"#!/bin/sh\necho {name} >> {self.marker}\n")
            p.chmod(0o755)

    def test_a_hand_built_environment_that_drops_the_redirect_is_refused(self):
        with self.assertRaises(_planeguard.AmbientGitConfig):
            subprocess.run([str(self.dir / "git"), "commit", "-m", "x"],
                           env={"PATH": os.environ["PATH"]})
        self.assertFalse(self.marker.exists(), "refused, and yet it ran")

    def test_an_empty_environment_is_refused_too(self):
        with self.assertRaises(_planeguard.AmbientGitConfig):
            subprocess.run([str(self.dir / "git"), "status"], env={})
        self.assertFalse(self.marker.exists())

    def test_redirecting_the_global_file_but_not_the_system_one_is_still_refused(self):
        """Half a redirect reads ``/etc/gitconfig``, which on a managed machine is where
        an organisation puts exactly the kind of setting this is about."""
        with self.assertRaises(_planeguard.AmbientGitConfig):
            subprocess.run([str(self.dir / "git"), "status"],
                           env={"GIT_CONFIG_GLOBAL": os.devnull})
        self.assertFalse(self.marker.exists())

    def test_a_bare_name_on_the_path_is_refused_too(self):
        """How a test actually spells it — `["git", "status"]`, letting `$PATH` resolve it —
        so a rule that only recognised full paths would miss nearly every real call."""
        with self.assertRaises(_planeguard.AmbientGitConfig):
            subprocess.run(["git", "status"], env={"PATH": str(self.dir)})
        self.assertFalse(self.marker.exists())

    def test_the_refusal_names_the_test_the_command_and_the_ways_out(self):
        with self.assertRaises(_planeguard.AmbientGitConfig) as caught:
            subprocess.run([str(self.dir / "git"), "commit", "-m", "x"], env={})
        text = str(caught.exception)
        self.assertIn("test_the_refusal_names_the_test_the_command_and_the_ways_out", text)
        self.assertIn("commit", text)
        self.assertIn("op-ssh-sign", text)          # what it costs
        self.assertIn("env=None", text)             # way out 1
        self.assertIn("os.environ", text)           # way out 2
        self.assertIn("_gitguard.environment()", text)   # way out 3
        for name in _gitguard.NAMES:                # way out 4, named rather than implied
            self.assertIn(name, text)

    def test_it_is_a_base_exception_so_a_fallback_cannot_eat_it(self):
        """`charter.planegit` and `charter.gitpolicy` both turn a failed git call into a
        degraded answer through `except Exception`, and a tripwire something catches
        reports a benign state instead of failing."""
        self.assertTrue(issubclass(_planeguard.AmbientGitConfig, BaseException))
        self.assertFalse(issubclass(_planeguard.AmbientGitConfig, Exception))

    def test_a_program_that_is_not_git_may_state_its_own_environment(self):
        """The boundary. This is a rule about ONE program's configuration, not a ban on
        building an environment: `test_cli_smoke` and `test_config` both launch charter
        itself with a hand-built one, on purpose."""
        subprocess.run([str(self.dir / "tar"), "--version"], env={})
        self.assertTrue(self.marker.exists())


class EveryWayOutOfTheRefusalWorks(unittest.TestCase):
    """A tripwire with no usable exit is deleted by whoever hits it next."""

    def setUp(self):
        self.dir = pathlib.Path(tempfile.mkdtemp(prefix="gitconfigout-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.marker = self.dir / "the-cli-ran"
        p = self.dir / "git"
        p.write_text(f"#!/bin/sh\necho git >> {self.marker}\n")
        p.chmod(0o755)

    def test_inheriting_this_processs_environment(self):
        subprocess.run([str(self.dir / "git"), "status"])
        self.assertTrue(self.marker.exists())

    def test_building_it_from_os_environ(self):
        subprocess.run([str(self.dir / "git"), "status"],
                       env={**os.environ, "GIT_AUTHOR_NAME": "someone"})
        self.assertTrue(self.marker.exists())

    def test_adding_the_helpers_own_answer_to_a_bare_environment(self):
        subprocess.run([str(self.dir / "git"), "status"],
                       env={"PATH": os.environ["PATH"], **_gitguard.environment()})
        self.assertTrue(self.marker.exists())

    def test_a_test_that_spells_its_own_devnull_redirect_is_not_refused(self):
        """Four modules here already write this, and one of them predates the redirect by
        months. A rule that insisted on the suite's own path would refuse a test for being
        MORE hermetic than it asks."""
        subprocess.run([str(self.dir / "git"), "status"],
                       env={"GIT_CONFIG_GLOBAL": os.devnull,
                            "GIT_CONFIG_SYSTEM": os.devnull})
        self.assertTrue(self.marker.exists())

    def test_and_the_nosystem_spelling_counts_as_the_system_half(self):
        subprocess.run([str(self.dir / "git"), "status"],
                       env={"GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"})
        self.assertTrue(self.marker.exists())


if __name__ == "__main__":
    unittest.main()
