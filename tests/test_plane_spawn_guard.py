"""The suite-wide tripwire that refuses to SPAWN a charter against the developer's plane.

The sibling of `test_plane_write_guard.py`, and it follows the same rule: every case here
installs a THROWAWAY directory as "the real plane" and asserts against that. Pointing a
case at the operator's actual plane to prove a spawn is refused would, the first time the
guard regressed, run `gl-refresh` and `persona _gc` against a live machine — which is the
accident this guard exists to prevent.

**How "it was refused" is told apart from "it silently did nothing".** The fake charter
below writes a marker file when it runs. A refused case asserts both that
:class:`RealPlaneSpawn` was raised AND that the marker is absent; the allowed case asserts
the marker is PRESENT. Without that second half, an absent marker would prove nothing —
"the child never ran" and "the child ran and could not write" look identical, which is a
failure mode this repo has shipped before.
"""

from __future__ import annotations

import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, hooks, root
from tests import _envguard, _planeguard


class WhatIsGuarded(unittest.TestCase):
    def test_popen_is_wrapped_on_the_class_at_package_import(self):
        """The CLASS, not the ``subprocess.Popen`` module attribute.

        `subprocess.run`/`check_output`/`check_call` construct the class directly, and so
        does anything that did ``from subprocess import Popen``. A wrapper on the module
        attribute would watch none of them.
        """
        self.assertEqual(getattr(subprocess.Popen.__init__, "__module__", None),
                         "tests._planeguard",
                         "Popen.__init__ is unwrapped — charter children spawn unseen")

    def test_the_guarded_root_is_this_machines_own_plane(self):
        """Armed against the plane the test PROCESS resolved, not some later one.

        Recomputed from `root.find_root` rather than read off `config.ROOT`, which any
        `PersonaIso` case may have repointed by the time this runs.

        ``$CHARTER_ROOT`` is declared unset rather than left ambient, which is what
        `_envguard` asks of any test that depends on it (#519). Saying so is not a
        formality here: the name was scrubbed at install precisely so this walk starts from
        the cwd, and a machine that had exported it would otherwise make this assert about
        the operator's shell.
        """
        _envguard.unset(root.ENV_VAR)
        self.assertIn(os.path.abspath(str(root.find_root())), _planeguard._REAL_ROOT)


#: A program that exists, runs nothing and exits non-zero — the stand-in interpreter for
#: the ``-m charter`` cases, so that a guard which failed to refuse would leave a failed
#: exit rather than run something. Looked up rather than written out: there is no
#: ``/bin/false`` on macOS, so the literal turned a regression in those cases into a
#: `FileNotFoundError` naming a path instead of a failure naming the guard.
_FALSE = shutil.which("false") or "/bin/false"


class _FakePlane(unittest.TestCase):
    """A throwaway plane installed as "the real plane", plus a fake ``charter`` binary."""

    def setUp(self):
        self.real = Path(tempfile.mkdtemp(prefix="spawnguard-real-"))
        self.addCleanup(shutil.rmtree, self.real, True)
        (self.real / root.MARKER).write_text("schema = 1\n")

        self.elsewhere = Path(tempfile.mkdtemp(prefix="spawnguard-else-"))
        self.addCleanup(shutil.rmtree, self.elsewhere, True)
        (self.elsewhere / root.MARKER).write_text("schema = 1\n")

        # A `charter` that leaves evidence. Named `charter` because that is what the guard
        # is looking for at the end of every spelling below, and it makes the "allowed"
        # case a real spawn of a real process rather than an assertion about a decision
        # function.
        self.marker = self.elsewhere / "the-child-ran"
        self.fake = self.elsewhere / "charter"
        self.fake.write_text(f"#!/bin/sh\necho ran > {self.marker}\n")
        self.fake.chmod(0o755)

        self.enterContext(mock.patch.object(
            _planeguard, "_REAL_ROOT",
            (str(self.real), str(self.real.resolve()))))

    def run_fake(self, **kw):
        p = subprocess.Popen([str(self.fake)], **kw)
        p.wait()
        return p


class ARefusedSpawn(_FakePlane):
    def test_a_charter_child_that_would_walk_onto_the_real_plane_is_refused(self):
        """No ``$CHARTER_ROOT``, cwd inside the real plane: exactly the 131 detached
        children #527 measured, which resolved the operator's plane by walking up from
        wherever the test happened to be running."""
        with self.assertRaises(_planeguard.RealPlaneSpawn) as caught:
            self.run_fake(cwd=self.real, env={k: v for k, v in os.environ.items()
                                              if k != root.ENV_VAR})
        self.assertFalse(self.marker.exists(),
                         "refused after delegating — the child ran anyway")
        self.assertIn("REFUSED", str(caught.exception))

    def test_a_charter_root_pointing_at_the_real_plane_is_refused(self):
        """Handing the plane across the boundary only helps if it is a DIFFERENT plane."""
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            self.run_fake(cwd=self.elsewhere,
                          env={**os.environ, root.ENV_VAR: str(self.real)})
        self.assertFalse(self.marker.exists())

    def test_a_dash_m_charter_argv_is_recognised_too(self):
        """`util.self_relaunch_argv`'s spelling — the one every self-relaunch site uses.

        The interpreter is :data:`_FALSE` so that a guard which failed to refuse would
        leave a non-zero exit rather than run anything.
        """
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            subprocess.Popen([_FALSE, "-P", "-m", "charter", "gl-refresh"],
                             cwd=self.real,
                             env={k: v for k, v in os.environ.items()
                                  if k != root.ENV_VAR})

    def test_the_message_names_the_test_and_both_ways_out(self):
        """A tripwire nobody can act on is a tripwire that gets deleted."""
        with self.assertRaises(_planeguard.RealPlaneSpawn) as caught:
            self.run_fake(cwd=self.real, env={k: v for k, v in os.environ.items()
                                              if k != root.ENV_VAR})
        msg = str(caught.exception)
        self.assertIn("ARefusedSpawn.test_the_message_names_the_test_and_both_ways_out",
                      msg)
        self.assertIn("CHARTER_ROOT", msg)      # way out (2): hand it a throwaway plane
        self.assertIn("maybe_spawn", msg)       # way out (1): do not spawn at all
        self.assertIn(str(self.real), msg)      # which plane it would have landed on

    def test_it_is_a_base_exception_so_charters_own_fallbacks_cannot_eat_it(self):
        """`glstate.maybe_spawn` and `update.maybe_spawn` both wrap their `Popen` in
        `except Exception: return`. A tripwire those can catch reports nothing at all."""
        self.assertTrue(issubclass(_planeguard.RealPlaneSpawn, BaseException))
        self.assertFalse(issubclass(_planeguard.RealPlaneSpawn, Exception))


class AnAllowedSpawn(_FakePlane):
    def test_a_child_handed_a_throwaway_plane_really_runs(self):
        """The other half of the evidence: this is what proves an absent marker above
        means "never ran" rather than "could not write"."""
        p = self.run_fake(cwd=self.elsewhere,
                          env={**os.environ, root.ENV_VAR: str(self.elsewhere)})
        self.assertEqual(p.returncode, 0)
        self.assertTrue(self.marker.exists())

    def test_a_child_whose_cwd_is_a_throwaway_plane_really_runs(self):
        """`$CHARTER_ROOT` is not the only isolation that works — a cwd inside another
        plane resolves that one, and `charter init` in a fresh directory resolves none."""
        p = self.run_fake(cwd=self.elsewhere,
                          env={k: v for k, v in os.environ.items()
                               if k != root.ENV_VAR})
        self.assertEqual(p.returncode, 0)
        self.assertTrue(self.marker.exists())

    def test_a_child_that_is_not_charter_is_never_examined(self):
        """The guard is about charter's own plane resolution. Refusing `git`, `tmux` or
        `sh` because they happen to run inside the checkout would make the suite
        unusable — and none of them reads a `charter.toml`."""
        out = subprocess.run([sys.executable, "-c", "print('hi')"], cwd=self.real,
                             capture_output=True, text=True,
                             env={k: v for k, v in os.environ.items()
                                  if k != root.ENV_VAR})
        self.assertEqual(out.stdout.strip(), "hi")


class EverySpellingThatReachesCharter(_FakePlane):
    """The question is "will this child resolve the operator's plane as charter", not "is
    this argv one of the spellings charter's own code uses".

    Round one asked the second question, and the difference was measured against this same
    ``_REAL_ROOT``: ``[python, "-m", "charter", "--version"]`` was refused, while
    ``[python, "-c", "from charter import config; print(config.ROOT)"]``, ``["/bin/sh",
    "-c", "<python> -m charter --version"]`` and the same command as a ``shell=True``
    string all RAN, against the real plane. Two of those were not hypothetical: nine
    charter-importing ``python -c`` children per suite run were being waved through, and a
    third site (`test_news_cross_process`) spawned a chain of them.

    Every case here is a REAL spawn attempt, with a canary the child would leave behind.
    An `assertRaises` alone would not tell "refused" from "refused after the child had
    already gone" — the failure mode this file's header names.
    """

    def setUp(self):
        super().setUp()
        self.canary = self.elsewhere / "the-child-ran-anyway"
        self.stranded = {k: v for k, v in os.environ.items() if k != root.ENV_VAR}

    def refuse(self, args, **kw):
        """Assert *args* is refused — and stay finite if it is not.

        Two things this does NOT do with an `assertRaises` block, both of them lessons from
        the suite's own open hangs (#545, #546). It never `wait()`s on a child it did not
        expect to exist: a regression that lets `watch -n 1 charter` through would otherwise
        park the whole run forever on a command that by definition never exits. And it hands
        every child ``/dev/null`` for stdin, so a `su` that reaches a real terminal asks
        nobody for a password. A guard's own test may not be the thing that makes a red run
        impossible to get.

        Evidence is cleared FIRST, because several of these run as `subTest`s inside one
        case and share one setUp: a marker left by an earlier spelling would fail every
        later one on evidence that is not about it, turning one red case into seven and
        hiding which spelling actually got through.
        """
        for evidence in (self.canary, self.marker):
            if evidence.exists():
                evidence.unlink()
        kw.setdefault("stdin", subprocess.DEVNULL)
        try:
            child = subprocess.Popen(args, cwd=self.real, env=self.stranded, **kw)
        except _planeguard.RealPlaneSpawn as refused:
            self.assertFalse(self.canary.exists(),
                             "refused after delegating — the child ran anyway")
            self.assertFalse(self.marker.exists())
            return str(refused)
        child.kill()
        child.wait()
        self.fail(f"not refused: {args!r} spawned a charter that would resolve "
                  f"{self.real} — the guarded plane")

    def test_a_python_dash_c_that_imports_charter(self):
        """The shape that was live in the suite. The write comes FIRST, so a guard that
        let this through leaves the canary whatever the import then does."""
        self.refuse([sys.executable, "-c",
                     f"open({str(self.canary)!r}, 'w').close()\nimport charter"])

    def test_a_dash_c_that_names_the_module_as_a_string(self):
        """``__import__("charter")`` reaches the same import by a different token."""
        self.refuse([sys.executable, "-c",
                     f"open({str(self.canary)!r}, 'w').close()\n__import__('charter')"])

    def test_a_shell_command_string(self):
        self.refuse(["/bin/sh", "-c", f"{self.fake} --version"])

    def test_the_same_command_with_shell_true(self):
        self.refuse(f"{self.fake} --version", shell=True)

    def test_a_command_substitution_inside_an_assignment(self):
        """`hooks/hooks.json`'s own shape — ``out="$(charter doctor 2>&1)" || …``. Every
        lexer reads that as an assignment, so the invocation is in no command position at
        all; it is one for `_substitution_bodies`."""
        self.refuse(["/bin/sh", "-c", f'out="$({self.fake} doctor 2>&1)" || true'])

    def test_a_quoted_command_substitution_that_is_not_an_assignment(self):
        """The same clause as above with the assignment taken away, and QUOTED — which is
        what leaves the substitution scan as the only reader of it.

        Unquoted, ``$(…)``'s parentheses are punctuation to `shlex` and the segmenter walks
        straight into the inner command on its own. Quoted, the whole substitution is one
        argument to `echo`, in no command position and in no assignment. Two guards that
        cover each other look like one guard until one of them is taken away.
        """
        self.refuse(["/bin/sh", "-c", f'echo "$({self.fake} --version)"'])

    def test_an_assignment_that_names_charter(self):
        """``c=charter; eval $c``. `eval`'s argument is a variable, so the wrapper clause
        sees no charter in it and the command word is `eval` rather than something
        computed: the assignment is the only place the name appears."""
        self.refuse(["/bin/sh", "-c", f"c={self.fake}; eval $c --version"])

    def test_a_command_word_the_shell_computes(self):
        """The name reaches the command position through the environment, and the only
        charter in the string itself is a COMMENT — which `shlex` strips before any reader
        sees it. What is left is ``$CBIN --version``, and a command word this cannot
        resolve is refused rather than guessed at."""
        self.stranded["CBIN"] = str(self.fake)
        self.refuse(["/bin/sh", "-c", "# this line starts charter\n$CBIN --version"])

    def test_a_wrapper_that_takes_a_command_as_its_arguments(self):
        """`nice` rather than `sudo`: if this ever stops being refused the fallout is the
        fake charter running, not a password prompt on a real machine.

        The ARGV form is the one that matters and it is `test_a_wrapper_in_an_argv…`
        below, not this one. This case wrote only the shell-string spelling for a round,
        and the shell string is the form that already had a wrapper clause — so the plain
        ``["nice", "charter", "docs"]`` went unexercised and ran.
        """
        self.refuse(["/bin/sh", "-c", f"nice {self.fake} --version"])

    def test_a_wrapper_in_an_argv_is_followed_to_the_program(self):
        """The plainest spelling there is, and it RAN: ``["nice", "charter", "docs"]``.

        `_cmd_launches_charter` read `nice` as "a command called nice" and stopped. The
        wrapper follow existed only inside shell STRINGS, while `_COMMAND_WRAPPERS`' own
        docstring justified itself with ``sudo charter doctor`` — an argv — and the case
        above pinned only the string form. A reason that names a shape the code cannot see
        is the shape this whole round was sent back over.

        Each of these is now answered by `charter.hooks._split_env_chdir`, production's own
        reader, including each wrapper's own option arity: `nice -n 10`, `timeout`'s bare
        duration, `xargs -n1`, `stdbuf -o0` glued, and `env -i`, which is the flag a flat
        "wrappers take a value" table gets wrong.
        """
        for argv in (["nice", str(self.fake), "--version"],
                     ["nice", "-n", "10", str(self.fake), "--version"],
                     ["nohup", str(self.fake), "--version"],
                     ["env", "-i", str(self.fake), "--version"],
                     ["stdbuf", "-o0", str(self.fake), "--version"],
                     ["timeout", "5", str(self.fake), "--version"],
                     ["timeout", "--preserve-status", "5", str(self.fake)],
                     ["xargs", "-n1", str(self.fake)],
                     ["nice", "-n", "10", "nohup", str(self.fake), "--version"]):
            with self.subTest(argv=argv):
                self.refuse(argv)

    def test_a_wrapper_whose_grammar_cannot_be_followed_refuses_on_the_word(self):
        """`flock <file> <cmd>` and `watch -n1 <cmd>` put a positional of their own in
        front of the program, so no reader that names "the program" can name charter here.

        These are the reason the wrapper clause survives alongside the production split
        rather than being replaced by it: where the program cannot be named, the word
        appearing ANYWHERE after the wrapper is what refuses. `su -c` is here for the same
        reason `eval` is — its argument is a command STRING, which production's Bash guard
        states as a limit it does not follow.
        """
        for argv in (["flock", str(self.elsewhere / "lock"), str(self.fake), "--version"],
                     ["watch", "-n", "1", str(self.fake), "--version"],
                     ["su", "-c", f"{self.fake} --version"]):
            with self.subTest(argv=argv):
                self.refuse(argv)

    def test_a_bundled_short_option_reaches_a_shells_command_string(self):
        """``bash -lc 'charter docs'`` is what a login shell is SPELLED, and it ran.

        `_python_launches_charter` already walked a bundle — that is how ``-Pmcharter`` was
        caught — while the shell branch matched ``tok == "-c"`` and ``tok.startswith("-c")``
        and saw none of `-lc`, `-ec`, `-xc`, `-ic`. One walk, `_bundled_option`, now serves
        both. `-o pipefail` is in here because it is the shell short option that takes a
        VALUE: a walk that read its value as more bundled letters would be the mirror of
        the `env -i` mistake production's per-wrapper table exists to avoid.
        """
        for argv in (["/bin/bash", "-lc", f"{self.fake} --version"],
                     ["/bin/sh", "-ec", f"{self.fake} --version"],
                     ["/bin/sh", "-xc", f"{self.fake} --version"],
                     ["/bin/sh", "-ic", f"{self.fake} --version"],
                     ["/bin/sh", f"-c{self.fake} --version"],
                     ["/bin/bash", "-o", "pipefail", "-c", f"{self.fake} --version"],
                     ["/bin/bash", "--login", "-c", f"{self.fake} --version"]):
            with self.subTest(argv=argv):
                self.refuse(argv)

    def test_env_split_string_is_not_a_way_through(self):
        """`env -S '<cmd>'` packs a whole command into ONE token, and it printed the
        guarded plane.

        The old reader skipped `-S` as a value-taking option, which left an EMPTY argv —
        and an empty argv was answered "not charter". Its own docstring had warned against
        exactly that (*"a guard guessing at an unknown option's arity would start skipping
        the command word itself"*) and then committed it on the one flag whose value IS the
        command word. Production said the same thing and got it right
        (`hooks._SPLIT_STRING_FLAGS`, pinned by
        `test_guard_parsing.test_env_split_string_is_not_a_way_through`, whose name this
        borrows deliberately), and that is the code this now calls.
        """
        for argv in (["env", "-S", f"{self.fake} --version"],
                     ["env", f"-S{self.fake} --version"],
                     ["env", f"--split-string={self.fake} --version"],
                     ["env", "-S", f"nice {self.fake} --version"],
                     ["env", "-S",
                      f"{sys.executable} -c 'from charter import config; print(config)'"]):
            with self.subTest(argv=argv):
                self.refuse(argv)

    def test_a_packed_command_that_itself_contains_an_equals_sign(self):
        """``env -Sfoo=1 charter docs``, which reusing production's reader let through.

        `hooks._split_env_chdir` reads the ``--split-string=`` spelling before the glued
        ``-S…`` one, so it splits this at the FIRST ``=`` — the program becomes ``1`` and
        the charter after it is never looked at. Reuse means inheriting that, so the
        ORDERING is repaired on the way in (`_unpack_split_strings`) using production's own
        flag names, and the repaired tokens go back to production's splitter.

        The same input is a live bypass of charter's Bash tool-gate on `main` — measured:
        ``env -Sfoo=1 cat .charter/vaults/x.json`` and ``env -Sfoo=1 charter secret get v k
        --reveal --force`` are both ALLOWED there while the unwrapped commands are denied.
        That is production's to fix (#547); this case is only about the harness not
        shipping the same hole.
        """
        for argv in ([str(self.fake), "docs"],
                     ["env", f"-Sfoo=1 {self.fake} docs"],
                     ["env", "-Sfoo=1", str(self.fake), "docs"],
                     ["env", f"--split-string=foo=1 {self.fake} docs"],
                     ["/bin/sh", "-c", f"env -Sfoo=1 {self.fake} docs"]):
            with self.subTest(argv=argv):
                self.refuse(argv)

    def test_the_name_glued_to_an_option_letter_still_reaches_the_lexer(self):
        """``env -Scharter --version``, as a shell STRING, and it is the GATE that misses.

        `-S` puts a word character immediately in front of the name, so the word test —
        which exists to keep ``<checkout>/charter/sub`` from reading as a command — answers
        "charter is not named here" about a string whose entire content is a charter
        invocation, and the string is never lexed at all. A gate that can only cause misses
        must not be the thing deciding: :data:`~tests._planeguard._CHARTER_MENTION` asks
        for the name and nothing about its boundaries, and the word test still decides what
        is a command once the lexer has run.
        """
        bindir = self.elsewhere / "onpath"
        bindir.mkdir()
        shutil.copy(self.fake, bindir / "charter")
        (bindir / "charter").chmod(0o755)
        self.stranded["PATH"] = f"{bindir}:{self.stranded.get('PATH', '')}"
        self.refuse(["/bin/sh", "-c", "env -Scharter --version"])

    def test_the_program_name_is_case_folded_too(self):
        """``["CHARTER", "docs"]`` ran, against a guarded plane.

        Same class as production's `test_the_program_name_is_case_folded_too`, and the same
        one-line answer: on the filesystems this runs on `CHARTER` and `charter` are one
        binary, so a guard that matches one casing has a Shift key for a bypass. The
        harness guard was the last one in the tree still comparing
        ``os.path.basename(parts[0]) == "charter"``.
        """
        upper = self.elsewhere / "CHARTER"
        # On a case-INSENSITIVE filesystem this IS `self.fake`, written back unchanged —
        # which is the whole point of the case: one file, two spellings, one binary.
        upper.write_text(self.fake.read_text())
        upper.chmod(0o755)
        self.refuse([str(upper), "docs"])
        self.refuse(["/bin/sh", "-c", f"{upper} docs"])
        self.refuse([_FALSE, "-m", "CHARTER", "gl-refresh"])

    def test_the_pre_rename_binary_is_charter_too(self):
        """`edm` is charter's former name, and a machine that still has it on `PATH` has a
        binary that resolves a plane. Production keeps it in `_CHARTER_PROGS` for exactly
        that reason; this reads the same constant rather than deciding again."""
        edm = self.elsewhere / "edm"
        edm.write_text(self.fake.read_text())
        edm.chmod(0o755)
        self.refuse([str(edm), "docs"])
        self.refuse(["nice", str(edm), "docs"])

    def test_an_interpreter_is_what_it_resolves_to_not_what_it_is_called(self):
        """``Popen([<symlink to python3>, "-c", "import charter"])`` ran.

        The guard read the LINK's name, found neither `python` nor `sys.executable`'s
        basename in it, and answered "not an interpreter". What the child execs is decided
        by the kernel, not by the spelling: a path is followed to what is really there, a
        RELATIVE path against the child's own cwd — `Popen` chdirs before it execs — and a
        bare name along the child's own ``PATH``.
        """
        link = self.elsewhere / "notpython"
        os.symlink(sys.executable, link)
        self.refuse([str(link), "-c", "import charter"])

        # relative: it has to live in the plane the child is given, because that is the
        # directory the name is resolved against.
        os.symlink(sys.executable, self.real / "notpython")
        self.refuse(["./notpython", "-c", "import charter"])

    def test_a_bare_name_is_looked_up_on_the_childs_own_path(self):
        """``Popen(["charter", "--version"], env={"PATH": …})`` — the spelling charter's own
        hooks use, with nothing but ``PATH`` deciding which binary runs."""
        bindir = self.elsewhere / "bin"
        bindir.mkdir()
        shutil.copy(self.fake, bindir / "charter")
        (bindir / "charter").chmod(0o755)
        self.stranded["PATH"] = f"{bindir}:{self.stranded.get('PATH', '')}"
        self.refuse(["charter", "--version"])
        self.refuse(["/bin/sh", "-c", f"PATH={bindir}:$PATH charter --version"])

    def test_an_env_wrapper_in_front_of_the_binary(self):
        """``env -u X <charter>``. The ``-m charter`` adjacency below would answer the
        other spelling on its own; nothing but reading past `env` answers this one."""
        self.refuse(["env", "-u", "NOTHING", str(self.fake), "--version"])

    def test_a_dash_c_body_that_will_not_tokenize(self):
        """Undecidable, and refusal is the direction that is safe to be wrong in.

        No canary here, and deliberately: source that does not compile runs nothing, so an
        absent canary would be true however this went. The refusal itself is the assertion.
        """
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            subprocess.Popen([sys.executable, "-c", "import ((("],
                             cwd=self.real, env=self.stranded).wait()

    def test_a_script_that_cannot_be_read(self):
        """Same rule, the other unreadable input. `python <gone>.py` fails either way; what
        must not happen is the guard deciding "not charter" because it could not look."""
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            subprocess.Popen([sys.executable, str(self.elsewhere / "gone.py")],
                             cwd=self.real, env=self.stranded).wait()

    def test_a_child_that_imports_the_test_package(self):
        """``import tests`` arms these guards, which imports `charter.config`, which
        resolves a plane — with the word never appearing. It was the last child in the
        suite still landing on the operator's plane.

        And it is the one child ``$CHARTER_ROOT`` cannot rescue: `_envguard` scrubs the
        charter namespace at import of this package, before `charter.config` loads, so the
        cwd is what decides. `test_no_test_reads_the_operators_shell` runs from a throwaway
        plane with ``$PYTHONPATH`` for exactly that reason.
        """
        msg = self.refuse([sys.executable, "-c",
                           f"open({str(self.canary)!r}, 'w').close()\nimport tests"])
        # And the message has to SAY so. A refusal whose stated remedy does not work for
        # the case it just refused is the shape this round was sent back over: a docstring
        # pointing at something the code cannot see.
        self.assertIn("cwd inside a throwaway plane", msg)
        self.assertIn("PYTHONPATH", msg)

    def test_a_shell_string_that_will_not_lex(self):
        """Undecidable, and refusal is the direction that is safe to be wrong in."""
        self.refuse(["/bin/sh", "-c", f'{self.fake} "--version'])

    def test_a_module_of_the_charter_package_run_by_path(self):
        """``python <plane>/charter/__main__.py``. Reading the file would not answer it —
        charter's real ``__main__.py`` reaches the CLI through ``from .cli import main``
        and never writes the word — so the package directory in the path is what does."""
        pkg = self.elsewhere / "tree" / "charter"
        pkg.mkdir(parents=True)
        (pkg / "__main__.py").write_text(f"open({str(self.canary)!r}, 'w').close()\n")
        self.refuse([sys.executable, str(pkg / "__main__.py")])

    def test_a_script_file_that_imports_charter(self):
        """A probe written to a temp file: nothing in its NAME says charter, and the guard
        reads it rather than guessing."""
        probe = self.elsewhere / "probe.py"
        probe.write_text(f"open({str(self.canary)!r}, 'w').close()\nimport charter\n")
        self.refuse([sys.executable, str(probe)])

    def test_an_env_wrapper_around_the_dash_m_spelling(self):
        """The suite's own gate is run as ``env -u CHARTER_SESSION_ID … python3 -m …``."""
        self.refuse(["env", "-u", "NOTHING", _FALSE, "-m", "charter", "gl-refresh"])

    def test_the_interpreter_is_named_by_executable_rather_than_argv(self):
        """``Popen(["x"], executable=".../charter")`` runs charter and says ``x``."""
        self.refuse(["not-charter", "--version"], executable=str(self.fake))

    def test_a_bare_path_object_as_the_whole_command(self):
        """`Popen` takes one path-like as the command. A `Path` is not iterable, so a
        reader that went straight to the sequence branch would get `TypeError` and answer
        "not charter" — the shape of every hole this recogniser has had."""
        self.refuse(self.fake)

    def test_cwd_and_env_are_read_when_they_are_passed_positionally(self):
        """`Popen`'s signature puts ``cwd`` ninth and ``env`` tenth. A guard that read only
        ``**kw`` would answer "no cwd, no env" here and ask `find_root` about this process
        instead — silently, and in the direction that allows."""
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            subprocess.Popen([str(self.fake)], -1, None, None, None, None, None, True,
                             False, str(self.real), self.stranded).wait()
        self.assertFalse(self.marker.exists())


class SpellingsThatAreNotASpawn(_FakePlane):
    """The other half, and the reason the recogniser lexes instead of grepping.

    A guard that refused on the word alone would be conservative in the direction that is
    safe — and would also refuse `test_toolgate`'s comparison of shlex's parse against a
    real bash's, whose corpus contains the STRING ``charter "sec"'ret' list v``, and every
    tmux fixture whose argv quotes a path inside the checkout. Each of these really runs.
    """

    def setUp(self):
        super().setUp()
        self.ran = self.elsewhere / "it-ran"
        self.stranded = {k: v for k, v in os.environ.items() if k != root.ENV_VAR}

    def allow(self, args, **kw):
        p = subprocess.Popen(args, cwd=self.real, env=self.stranded, **kw)
        self.assertEqual(p.wait(), 0)
        self.assertTrue(self.ran.exists(), "the child did not run")
        return self.ran.read_text()

    def test_charter_as_an_argument_to_another_command(self):
        """`test_toolgate` runs exactly this: bash, asked to echo a corpus entry back."""
        self.assertEqual(
            self.allow(["/bin/sh", "-c", f"printf %s charter > {self.ran}"]), "charter")

    def test_a_path_that_merely_contains_the_word(self):
        """``cd ~/IdeaProjects/charter/tests`` is not a charter spawn, and the suite is
        full of children whose argv names a path inside the checkout."""
        inside = self.elsewhere / "tree" / "charter"
        inside.mkdir(parents=True)
        self.assertEqual(
            self.allow(["/bin/sh", "-c", f"ls -d {inside} > {self.ran}"]).strip(),
            str(inside))

    def test_a_python_child_whose_only_charter_is_inside_a_string_literal(self):
        """`tokenize` is what tells a NAME from a path: `test_frame_tmux_integration`
        spawns ``python -c "open('<checkout>/…', 'w').close()"`` as a hostile-argv fixture,
        and it has no charter import in it."""
        path = self.elsewhere / "tree" / "charter" / "canary"
        path.parent.mkdir(parents=True)
        self.allow([sys.executable, "-c",
                    f"open({str(path)!r}, 'w').close()\n"
                    f"open({str(self.ran)!r}, 'w').write('ok')"])
        self.assertTrue(path.exists())

    def test_a_wrapped_command_whose_argument_is_a_path_inside_the_checkout(self):
        """Where the word test's shape earns itself. ``charter`` followed by ``/`` is a
        directory on the way to something else, not a command — so ``nice ls -d
        <checkout>/charter/sub`` is a wrapper carrying a PATH, and a regex that stopped one
        character earlier would refuse it as a wrapper carrying charter."""
        inside = self.elsewhere / "tree" / "charter" / "sub"
        inside.mkdir(parents=True)
        self.assertEqual(
            self.allow(["/bin/sh", "-c", f"nice ls -d {inside} > {self.ran}"]).strip(),
            str(inside))

    def test_a_shell_string_with_no_mention_of_charter_is_never_lexed(self):
        # `.resolve()`, because macOS's ``/tmp`` is a symlink and ``pwd`` reports the
        # resolved spelling — the same two-spellings problem `_REAL_ROOT` carries.
        self.assertEqual(self.allow(["/bin/sh", "-c", f'pwd > "{self.ran}"']).strip(),
                         str(self.real.resolve()))


class TheHarnessGuardIsNotASecondCopyOfProductions(unittest.TestCase):
    """The finding this round closed was not "a spelling was missing". It was that the
    harness guard held its OWN table of wrappers, its OWN `env` option arity and its OWN
    case rule, and each of them was weaker than the one `charter/hooks.py` already had —
    against inputs production denies and `tests/test_guard_parsing.py` already pins.

    Measured: ``["nice", "charter", "docs"]``, ``["CHARTER", "docs"]`` and ``["env", "-S",
    "<python> -c 'from charter import config; print(config.ROOT)'"]`` all ran against a
    guarded plane, and the last printed it. `nice cat <vault>`, `CHARTER secret get …
    --reveal` and `env -S 'cat <vault>'` are all denied by production.

    So the cases below are not about a list of commands. They are the join: what production
    can parse, this parses, because it is the same code. A second copy that starts drifting
    fails here rather than in someone's plane.
    """

    def test_the_launcher_split_is_productions_own_reader(self):
        """Not "behaves like" — IS. If someone reinstates a private walk, this catches it
        by watching production's function get called."""
        with mock.patch.object(hooks, "_split_env_chdir",
                               wraps=hooks._split_env_chdir) as split:
            _planeguard._launcher_argv(["nice", "charter", "docs"])
        split.assert_called_once()

    def test_every_wrapper_production_follows_is_followed_here(self):
        """The whole of `hooks._WRAPPERS`, as an ARGV — the form that was unexercised.

        A decision-function table rather than a spawn table on purpose: `sudo` and `su`
        would prompt a real machine for a password, and the point being pinned is the
        JOIN with production's constant, which no amount of spawning would show.
        """
        for wrapper in sorted(hooks._WRAPPERS):
            with self.subTest(wrapper=wrapper):
                self.assertTrue(
                    _planeguard._cmd_launches_charter([wrapper, "charter", "docs"]),
                    f"`{wrapper} charter docs` is a charter spawn to charter's own Bash "
                    f"guard and not to this one — the two tables have drifted")

    def test_the_wrappers_this_adds_are_the_ones_production_has_no_use_for(self):
        """An addition is fine; a re-statement is not. Whatever is here beyond production's
        set has to be a wrapper whose argument grammar cannot be followed to a program —
        which is why they are refused on the WORD instead."""
        self.assertLessEqual(hooks._WRAPPERS, _planeguard._COMMAND_WRAPPERS)
        self.assertEqual(_planeguard._COMMAND_WRAPPERS - hooks._WRAPPERS,
                         {"eval", "su", "watch", "script", "flock"})

    def test_the_charter_word_is_built_from_productions_names(self):
        """Including `edm`, charter's pre-rename binary, and folded — the two things
        `hooks._is_charter` and `_VAULT_PATH_RE` were already folded for."""
        for name in hooks._CHARTER_PROGS:
            with self.subTest(name=name):
                self.assertTrue(_planeguard._CHARTER_WORD.search(f"{name} --version"))
                self.assertTrue(
                    _planeguard._CHARTER_WORD.search(f"{name.upper()} --version"))
        # …and still not a path that merely contains the word, which is the shape the
        # frame's tmux fixtures spawn by the dozen.
        self.assertFalse(
            _planeguard._CHARTER_WORD.search("cd ~/IdeaProjects/charter/tests"))

    #: `test_guard_parsing`'s own wrapper corpus, with `charter` where the vault path was.
    #: Every one of these is a command charter's Bash guard already names as charter.
    SAME_CORPUS = ("sudo charter doctor",
                   "env charter doctor",
                   "/usr/bin/env charter doctor",
                   "nice -n 10 charter doctor",
                   "stdbuf -o0 charter doctor",
                   "timeout 5 charter doctor",
                   "timeout -s KILL 5 charter doctor",
                   "env -i charter doctor",
                   "env -u PATH charter doctor",
                   "sudo -u root charter doctor",
                   "sudo -- charter doctor",
                   "sudo env charter doctor",
                   "xargs charter doctor",
                   "env FOO=bar charter doctor",
                   "env -S 'charter doctor'",
                   "env -Scharter doctor",
                   "env --split-string='charter doctor'",
                   "CHARTER doctor")

    def test_the_same_corpus_is_charter_to_both_guards(self):
        """Asked as an ARGV, which is the form that had no wrapper follow at all — a
        `Popen(["nice", "charter", "docs"])` never becomes a shell string on the way."""
        for cmd in self.SAME_CORPUS:
            with self.subTest(cmd=cmd):
                argv = shlex.split(cmd)
                self.assertTrue(hooks._is_charter(*hooks._split_env(argv)[::2]),
                                f"{cmd} is not charter to PRODUCTION — wrong corpus")
                self.assertTrue(_planeguard._cmd_launches_charter(argv), cmd)

    def test_the_same_corpus_is_charter_inside_a_shell_string(self):
        """And again one layer of quoting in, where `charter workspace _reconcile` and
        `out="$(charter doctor 2>&1)"` — charter's own `hooks.json` — actually live."""
        for cmd in (*self.SAME_CORPUS,
                    "if true; then charter doctor; fi",
                    'out="$(charter doctor 2>&1)" || true'):
            with self.subTest(cmd=cmd):
                self.assertTrue(_planeguard._shell_launches_charter(cmd), cmd)


class NoCharterEscapesThroughTheExecFamily(unittest.TestCase):
    """The guard watches `subprocess.Popen.__init__`, and nothing else starts a process.

    `os.execv*`, `os.posix_spawn*`, `os.spawn*` and `os.system` all go around it. Wrapping
    them too would be the obvious move and the wrong one: `execvp` REPLACES this process,
    so a wrapper that refused would be refusing something that cannot be a child of the
    suite at all, and `os.system` is not used here.

    What makes "nothing reaches charter that way" a fact rather than an assumption is this
    case. Every module in `charter/` and `tests/` is parsed, every call to one of those
    functions is found, and each has to be one this file has already looked at and written
    down. A fourth appears the day somebody writes one, named by file and line.

    Parsed rather than grepped, so that ``mock.patch("os.execvp")`` — which
    `test_frame_launcher` writes fifteen times — is what it is: a string, not a call.
    """

    #: ``module:function`` → why this one cannot start a charter. Frozen deliberately: the
    #: point is that a NEW exec is noticed, and a set that grew by itself would notice
    #: nothing.
    KNOWN = {
        "charter/commands_frame.py:execvp":
            "`bypass` hands this process to the HARNESS (`claude`), and replaces it. The "
            "thing that runs afterwards is not charter and has no plane to resolve.",
        "charter/commands_secrets.py:execvpe":
            "`secret exec` replaces this process with the operator's own command. If they "
            "type `charter`, the process that becomes charter is the one that was already "
            "running — a test doing this by accident loses the runner, not a plane.",
        "tests/test_frame_tmux_integration.py:execvp":
            "`tmux attach` inside a `pty.fork` child, which `os._exit`s in its `finally`.",
    }

    _WATCHED = ("execl", "execle", "execlp", "execlpe", "execv", "execve", "execvp",
                "execvpe", "posix_spawn", "posix_spawnp", "spawnl", "spawnle", "spawnlp",
                "spawnlpe", "spawnv", "spawnve", "spawnvp", "spawnvpe", "system")

    def test_every_exec_in_the_tree_is_one_that_cannot_become_charter(self):
        import ast

        tree = Path(__file__).resolve().parent.parent
        found = {}
        for path in sorted((*tree.glob("charter/**/*.py"), *tree.glob("tests/**/*.py"))):
            try:
                parsed = ast.parse(path.read_text(encoding="utf-8"))
            except (OSError, SyntaxError):       # not this case's business
                continue
            for node in ast.walk(parsed):
                if not isinstance(node, ast.Call):
                    continue
                func = node.func
                if (isinstance(func, ast.Attribute) and func.attr in self._WATCHED
                        and isinstance(func.value, ast.Name) and func.value.id == "os"):
                    key = f"{path.relative_to(tree)}:{func.attr}"
                    found.setdefault(key, node.lineno)

        unexpected = {k: v for k, v in found.items() if k not in self.KNOWN}
        self.assertEqual(
            unexpected, {},
            f"a process is started by a call `tests._planeguard` does not watch: "
            f"{unexpected}. `RealPlaneSpawn` wraps `subprocess.Popen.__init__` only, so a "
            f"charter started this way reaches the operator's plane unseen. Either it "
            f"cannot become charter — say why, and add it to "
            f"`NoCharterEscapesThroughTheExecFamily.KNOWN` — or it can, and it should go "
            f"through `subprocess` instead.")

        gone = set(self.KNOWN) - set(found)
        self.assertEqual(gone, set(),
                         f"{gone} is written down here and no longer exists — an "
                         f"allow-list nobody prunes stops being read")


class HowTheChildsPlaneIsResolved(_FakePlane):
    def test_it_asks_find_root_with_the_childs_environment_and_cwd(self):
        """Not with this process's. The whole defect is that the child's answer differs
        from the parent's, so a guard that asked the parent's question would agree with
        every spawn it was meant to catch.
        """
        seen = {}

        def spy(start=None, env=None):
            seen["start"], seen["env"] = start, env
            return self.elsewhere

        with mock.patch.object(root, "find_root", side_effect=spy):
            self.run_fake(cwd=self.elsewhere,
                          env={**os.environ, root.ENV_VAR: str(self.elsewhere)})
        self.assertEqual(Path(seen["start"]), self.elsewhere)
        self.assertEqual(seen["env"].get(root.ENV_VAR), str(self.elsewhere))

    def test_a_child_with_no_env_of_its_own_is_asked_about_ours(self):
        """``env=None`` means the child inherits this process's environment, so that is
        the environment its answer depends on.

        A COPY of it, not the mapping itself, and the difference is the other tripwire in
        this package: `_envguard` refuses an undeclared targeted read of ``$CHARTER_ROOT``
        and leaves bulk reads alone, so handing `find_root` the live `os.environ` would
        charge every test that spawns a `bash` with a read it never made. The values are
        the same either way — the ambient ones were scrubbed at install — and a copy is
        exactly what the child inherits.
        """
        seen = {}

        def spy(start=None, env=None):
            seen["env"] = env
            return self.elsewhere

        with mock.patch.object(root, "find_root", side_effect=spy):
            self.run_fake(cwd=self.elsewhere)
        self.assertIsNot(seen["env"], os.environ)
        self.assertEqual(seen["env"], dict(os.environ))

    def test_an_unresolvable_child_falls_back_to_its_own_cwd(self):
        """`charter.config` calls `find_root_or_cwd`, which falls back to the working
        directory when no plane is found — so a child with a bad `$CHARTER_ROOT` and a cwd
        of the real plane still lands on it. Answering "no plane, allow" there would be
        the guard waving through the one case the fallback makes dangerous.
        """
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            self.run_fake(cwd=self.real,
                          env={**os.environ, root.ENV_VAR: str(self.elsewhere / "gone")})
        self.assertFalse(self.marker.exists())


class WhatCharterHandsItsOwnChildren(unittest.TestCase):
    """`util.child_env` — the reason an isolated case satisfies the guard without knowing
    it exists."""

    def setUp(self):
        self.plane = Path(tempfile.mkdtemp(prefix="spawnguard-plane-"))
        self.addCleanup(shutil.rmtree, self.plane, True)

    def test_a_child_is_told_the_plane_this_process_resolved(self):
        from charter import util
        with mock.patch.object(config, "ROOT", self.plane), \
                mock.patch.object(config, "HAS_CONTROL_PLANE", True):
            self.assertEqual(util.child_env()[root.ENV_VAR], str(self.plane))

    def test_an_inherited_pointer_does_not_win_over_the_resolved_plane(self):
        """`config.ROOT` is the plane this process is actually reading and writing. When
        the two disagree, the child's job is to agree with its parent, not with a shell
        variable the parent already declined to follow."""
        from charter import util
        with mock.patch.object(config, "ROOT", self.plane), \
                mock.patch.object(config, "HAS_CONTROL_PLANE", True), \
                mock.patch.dict(os.environ, {root.ENV_VAR: "/somewhere/else"},
                                clear=False):
            self.assertEqual(util.child_env()[root.ENV_VAR], str(self.plane))

    def test_a_planeless_process_hands_nothing(self):
        """`$CHARTER_ROOT` wins outright in `find_root` and a value with no `charter.toml`
        at it RAISES rather than falling back — so a pointer to a non-plane is worse than
        no pointer. The spawners refuse to fork at all in that state; this pins that
        `child_env` would not have lied to the child either."""
        from charter import util
        with mock.patch.object(config, "ROOT", self.plane), \
                mock.patch.object(config, "HAS_CONTROL_PLANE", False), \
                mock.patch.dict(os.environ, {}, clear=True):
            self.assertNotIn(root.ENV_VAR, util.child_env())


class NoBackgroundRefreshWithoutAPlane(unittest.TestCase):
    """Both best-effort refreshers return before forking when there is no plane.

    Not a tidiness rule: outside a plane `config.STATE_DIR` is ``<cwd>/.charter``, so the
    child would scatter charter's caches into whatever directory the render ran in — and,
    having been handed no plane, would go looking for one of its own.
    """

    def test_glstate_does_not_fork(self):
        from charter import glstate
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False), \
                mock.patch.object(glstate.subprocess, "Popen") as popen:
            glstate.maybe_spawn([Path("/tmp")])
        popen.assert_not_called()

    def test_update_does_not_fork(self):
        from charter import update
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False), \
                mock.patch.object(update.subprocess, "Popen") as popen:
            update.maybe_spawn()
        popen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
