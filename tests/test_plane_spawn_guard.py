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
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, root
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

        The interpreter is `/bin/false` so that a guard which failed to refuse would leave
        a non-zero exit rather than run anything.
        """
        with self.assertRaises(_planeguard.RealPlaneSpawn):
            subprocess.Popen(["/bin/false", "-P", "-m", "charter", "gl-refresh"],
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
        with self.assertRaises(_planeguard.RealPlaneSpawn) as caught:
            subprocess.Popen(args, cwd=self.real, env=self.stranded, **kw).wait()
        self.assertFalse(self.canary.exists(),
                         "refused after delegating — the child ran anyway")
        self.assertFalse(self.marker.exists())
        return str(caught.exception)

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
        fake charter running, not a password prompt on a real machine. `nice` also has an
        argument grammar this deliberately does not follow — the word appearing anywhere
        after a wrapper is what refuses."""
        self.refuse(["/bin/sh", "-c", f"nice {self.fake} --version"])

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
        self.refuse(["env", "-u", "NOTHING", "/bin/false", "-m", "charter", "gl-refresh"])

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
