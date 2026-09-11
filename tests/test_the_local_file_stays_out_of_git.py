"""`charter.local.toml` stays out of git, and charter checks that rather than hoping.

A profile's command runs on a click, with no harness permission prompt in between. In a
committed file a merged PR or a chat could change that command and have it run on every
machine, so profiles live only in `charter.local.toml`, and "ignored" is guaranteed:
`init` writes `/charter.local.toml` into `.gitignore`, `reinit` backfills it, and a file git
tracks or would commit has its profiles refused (spec, *Where profiles live*).

The check is one `git --no-optional-locks status` under `LC_ALL=C`. A plain `status` takes
`index.lock` and would break a concurrent `charter save` (#917, ruling 34); git's "not a git
repository", the one failure that passes, is English only under `C` (ruling 32). Anything
else git cannot answer, a timeout included, is not a pass (review 4).
"""

from __future__ import annotations

import inspect
import io
import os
import shutil
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, doctor, instance, profiles, util
from charter.doctor import OK, WARN
from tests import _gitguard, _planeguard
from tests._isolation import PersonaIso
from tests.test_init import InitIso

_OK = """
[harness.ok]
kind = "claude"
command = ["claude"]
"""

#: Every rule a fresh `.gitignore` carried before this feature, so an append is the
#: profiles line and nothing else.
_BEFORE = "/workspaces/*/*\n!/workspaces/.gitkeep\n/.charter/\n/.claude/settings.local.json\n"


def _git(root: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True,
                   env={**os.environ, **_gitguard.environment()})


def _answer(rc: int, out: str = "", err: str = "") -> subprocess.CompletedProcess:
    return subprocess.CompletedProcess(["git"], rc, out, err)


class InitAndReinitIgnoreIt(InitIso):
    def setUp(self) -> None:
        super().setUp()
        self.addCleanup(shutil.rmtree, self.root, True)
        # `init` and `reinit` install opencode's shim where opencode reads it for every
        # project, `$XDG_CONFIG_HOME`, so it is stated here and never the operator's own.
        xdg = Path(tempfile.mkdtemp(prefix="charter-xdg-"))
        self.addCleanup(shutil.rmtree, xdg, True)
        self.enterContext(mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(xdg)}))
        self.said = io.StringIO()
        self.enterContext(redirect_stdout(self.said))
        self.enterContext(redirect_stderr(self.said))

    def _gitignore(self) -> Path:
        return self.root / ".gitignore"

    def _reinit(self) -> int:
        (self.root / "charter.toml").write_text(f"schema = {instance.SCHEMA}\n")
        config.use(self.root)            # `point_config_at`'s snapshot still restores
        return commands.cmd_reinit(SimpleNamespace())

    def test_a_fresh_plane_ignores_the_local_file(self):
        self.assertEqual(self._init(), 0)
        self.assertIn("/charter.local.toml", self._gitignore().read_text().splitlines())

    def test_an_existing_gitignore_gains_the_line_once(self):
        self._gitignore().write_text(_BEFORE)
        self.assertTrue(commands._ensure_gitignore(self.root))
        body = self._gitignore().read_text()
        self.assertEqual(body.splitlines().count("/charter.local.toml"), 1)
        self.assertFalse(commands._ensure_gitignore(self.root))
        self.assertEqual(self._gitignore().read_text(), body)

    def test_a_rule_that_merely_contains_the_name_does_not_count(self):
        self._gitignore().write_text(_BEFORE + "build/charter.local.toml.bak\n")
        commands._ensure_gitignore(self.root)
        self.assertIn("/charter.local.toml", self._gitignore().read_text().splitlines())

    def test_reinit_adds_it_to_a_plane_made_before_it(self):
        self._gitignore().write_text(_BEFORE)
        self.assertEqual(self._reinit(), 0)
        self.assertIn("/charter.local.toml", self._gitignore().read_text().splitlines())
        self.assertIn("/charter.local.toml", self.said.getvalue())

    def test_reinit_leaves_a_plane_that_has_it_alone(self):
        """Pin: it passes where reinit never touches `.gitignore`, and keeps the additive
        rule now that it does."""
        self._gitignore().write_text(_BEFORE + "/charter.local.toml\n")
        before = self._gitignore().read_bytes()
        self._reinit()
        self.assertEqual(self._gitignore().read_bytes(), before)
        self.assertNotIn("charter.local.toml", self.said.getvalue())

    def test_the_ignore_line_names_the_file_profiles_reads(self):
        """`LOCAL_PROFILES_IGNORE` spells the name instead of importing `charter.profiles`
        (ruling 43), so this is what keeps the ignored file and the read file the same one."""
        from charter import profiles
        self.assertEqual(commands.LOCAL_PROFILES_IGNORE, "/" + profiles.LOCAL_FILE)

    def test_the_backfill_says_whether_it_wrote(self):
        """`reinit` reports what it changed, not what it asked for (ADR 0013)."""
        self._gitignore().write_text(_BEFORE)
        self.assertTrue(commands._ensure_local_profiles_ignored(self.root))
        self.assertFalse(commands._ensure_local_profiles_ignored(self.root))


class TheFileMustBeIgnoredToBeUsed(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        _git(config.ROOT, "init", "-q")

    def _declare(self, root: Path | None = None) -> None:
        ((root or config.ROOT) / "charter.local.toml").write_text(_OK)

    def _not_a_repository(self) -> Path:
        plane = Path(tempfile.mkdtemp(prefix="charter-not-a-repo-")).resolve()
        self.addCleanup(shutil.rmtree, plane, True)
        # git walks upward looking for a repository. Stated, so a temp directory that sits
        # inside one on somebody's machine is not read as belonging to it.
        self.enterContext(mock.patch.dict(os.environ,
                                          {"GIT_CEILING_DIRECTORIES": str(plane.parent)}))
        self._declare(plane)
        return plane

    def test_an_absent_file_needs_no_git(self):
        """A file that is not there declares nothing, so there is nothing to refuse."""
        with mock.patch("charter.util.run",
                        side_effect=AssertionError("git ran for a file that is not there")):
            self.assertEqual(profiles.ignored_refusal(config.ROOT), "")

    def test_an_ignored_untracked_file_is_fine(self):
        (config.ROOT / ".gitignore").write_text("/charter.local.toml\n")
        self._declare()
        self.assertEqual(profiles.ignored_refusal(config.ROOT), "")

    def test_a_file_git_would_commit_is_refused(self):
        self._declare()
        self.assertIn("charter reinit adds", profiles.ignored_refusal(config.ROOT))

    def test_a_tracked_file_is_refused_even_when_ignored(self):
        """An ignore rule does not untrack a file: the next commit still carries it."""
        (config.ROOT / ".gitignore").write_text("/charter.local.toml\n")
        self._declare()
        _git(config.ROOT, "add", "-f", "charter.local.toml")
        _git(config.ROOT, "commit", "-q", "-m", "a profile committed by mistake")
        self.assertIn("git rm --cached", profiles.ignored_refusal(config.ROOT))

    def test_a_plane_that_is_not_a_repository_is_not_refused(self):
        self.assertEqual(profiles.ignored_refusal(self._not_a_repository()), "")

    def test_git_saying_not_a_repository_is_the_only_pass(self):
        """Review 4: `git_ignores` reads every non-zero answer as "not a repository", which
        its callers depend on and this check must not."""
        self._declare()
        cases = (
            (128, "fatal: not a git repository (or any of the parent directories): .git\n", ""),
            (128, "fatal: detected dubious ownership in repository at '/x'\n", "could not say"),
            (1, "fatal: not a git repository\n", "could not say"),
        )
        for rc, err, expected in cases:
            with self.subTest(rc=rc, err=err), \
                 mock.patch("charter.util.run", return_value=_answer(rc, "", err)):
                said = profiles.ignored_refusal(config.ROOT)
                if expected:
                    self.assertIn(expected, said)
                else:
                    self.assertEqual(said, "")

    def test_the_reason_git_gave_is_named(self):
        self._declare()
        with mock.patch("charter.util.run",
                        return_value=_answer(128, "", "fatal: detected dubious ownership\n")):
            self.assertIn("dubious ownership", profiles.ignored_refusal(config.ROOT))

    def test_a_git_that_fails_saying_nothing_is_named_by_its_exit(self):
        """`git_path_state` never raises (its docstring), and a git that failed in silence is
        still an answer to name: git exited, and with what. The sweep of `ce7f5d8` collapsed
        the `else` to `said.splitlines()[0]`, which raises on an empty stderr, and dropped the
        `or ""`, which raises when nothing was captured at all."""
        self._declare()
        for err in ("", None):
            with self.subTest(err=err), \
                 mock.patch("charter.util.run", return_value=_answer(1, "", err)):
                self.assertEqual(util.git_path_state(config.ROOT, "charter.local.toml"),
                                 (util.UNKNOWN_GIT, "git exited 1"))

    def test_the_ignore_check_takes_no_index_lock(self):
        """Ruling 34: this runs at launch, in the pane, in the selector, in `harness list`
        and in doctor, often enough to break a concurrent commit on a plain `status`."""
        self._declare()
        with mock.patch("charter.util.run",
                        return_value=_answer(0, "!! charter.local.toml\n")) as run:
            profiles.ignored_refusal(config.ROOT)
        self.assertEqual(run.call_args.args[0][:5],
                         ["git", "--no-optional-locks", "-C", str(config.ROOT), "status"])
        self.assertIsNotNone(run.call_args.kwargs.get("timeout"))

    def test_one_git_call_answers_every_state(self):
        """Re-review N8: one `status` tells committable, ignored and tracked apart."""
        real = util.run

        def ask() -> str:
            with mock.patch("charter.util.run", side_effect=real) as run:
                state, _why = util.git_path_state(config.ROOT, "charter.local.toml")
            self.assertEqual(run.call_count, 1)
            return state

        self._declare()
        self.assertEqual(ask(), util.COMMITTABLE)
        (config.ROOT / ".gitignore").write_text("/charter.local.toml\n")
        self.assertEqual(ask(), util.IGNORED)
        _git(config.ROOT, "add", "-f", "charter.local.toml")
        _git(config.ROOT, "commit", "-q", "-m", "tracked")
        self.assertEqual(ask(), util.TRACKED)
        (config.ROOT / "charter.local.toml").write_text(_OK + "# changed\n")
        self.assertEqual(ask(), util.TRACKED)

    def test_a_non_english_locale_still_reads_not_a_repository(self):
        """Re-review N8: git's own sentence is the only thing that tells "not a repository"
        from every other rc 128, and it is only English under `LC_ALL=C`."""
        plane = self._not_a_repository()
        real = util.run
        env = {"PATH": os.environ["PATH"], **_gitguard.environment(),
               "GIT_CEILING_DIRECTORIES": str(plane.parent),
               "LANG": "de_DE.UTF-8", "LC_ALL": "de_DE.UTF-8"}
        with mock.patch.dict(os.environ, env, clear=True), \
             mock.patch("charter.util.run", side_effect=real) as run:
            self.assertEqual(profiles.ignored_refusal(plane), "")
        self.assertEqual(run.call_args.kwargs["env"]["LC_ALL"], "C")

    def test_git_missing_is_not_a_pass(self):
        self._declare()
        with mock.patch("charter.util.run",
                        side_effect=FileNotFoundError(2, "No such file or directory", "git")):
            self.assertIn("could not say", profiles.ignored_refusal(config.ROOT))

    def test_a_git_that_times_out_is_not_a_pass_and_does_not_raise(self):
        self._declare()
        with mock.patch("charter.util.run",
                        side_effect=util.ProcTimeout(["git", "status"], 5.0)):
            self.assertIn("could not say", profiles.ignored_refusal(config.ROOT))

    def test_an_unparseable_status_is_not_a_pass(self):
        self._declare()
        with mock.patch("charter.util.run", return_value=_answer(0, "?! charter.local.toml\n")):
            self.assertIn("could not say", profiles.ignored_refusal(config.ROOT))

    def test_a_tracked_line_wins_over_an_ignored_one(self):
        """Measured on git 2.50.1: after `git rm --cached` and before the commit, status
        prints `D ` and `!!` for the one path. The next commit still carries it."""
        self._declare()
        with mock.patch("charter.util.run",
                        return_value=_answer(0, "D  charter.local.toml\n!! charter.local.toml\n")):
            self.assertIn("git rm --cached", profiles.ignored_refusal(config.ROOT))

    def test_untracking_takes_a_commit_before_the_file_passes(self):
        """F3, on real git. The plan's fix — `git rm --cached`, then `charter reinit` — never
        reaches a pass: until the removal is committed, status prints `D ` beside `!!`, which
        the next commit still carries. So the sentence names the commit, and this walks the
        whole fix to the pass."""
        (config.ROOT / ".gitignore").write_text("/charter.local.toml\n")
        self._declare()
        _git(config.ROOT, "add", "-f", ".gitignore", "charter.local.toml")
        _git(config.ROOT, "commit", "-q", "-m", "a profile committed by mistake")
        self.assertIn("commit that removal", profiles.ignored_refusal(config.ROOT))
        _git(config.ROOT, "rm", "-q", "--cached", "charter.local.toml")
        self.assertIn("git rm --cached", profiles.ignored_refusal(config.ROOT),
                      "a staged removal is still carried by the next commit")
        _git(config.ROOT, "commit", "-q", "-m", "untrack the profiles")
        commands._ensure_local_profiles_ignored(config.ROOT)
        self.assertEqual(profiles.ignored_refusal(config.ROOT), "")

    def test_the_existing_callers_see_git_ignores_unchanged(self):
        """Pin. `commands_secrets` and `doctor.check_credential_paths` read `None` as "not a
        repository"; a timeout added there would raise into both (review 4)."""
        self.assertNotIn("timeout", inspect.signature(util.git_ignores).parameters)
        with mock.patch("charter.util.run",
                        return_value=_answer(128, "", "fatal: detected dubious ownership\n")):
            self.assertIsNone(util.git_ignores(config.ROOT, "charter.local.toml"))


class DoctorWarns(PersonaIso):
    """The row reads the file as it is now: `config` was derived in `setUp`, before any of
    these files existed, so a row reading a value derived with `config` (as `config.PROFILES`
    was, before ruling 43 removed it) would pass every one of them."""

    def setUp(self) -> None:
        super().setUp()
        _git(config.ROOT, "init", "-q")

    def _row(self, text: str, *, ignored: bool = True) -> doctor.Result:
        if ignored:
            (config.ROOT / ".gitignore").write_text("/charter.local.toml\n")
        (config.ROOT / "charter.local.toml").write_text(text)
        return doctor.check_harness_profiles()

    def test_a_committable_local_file_is_a_warning_naming_reinit(self):
        r = self._row(_OK, ignored=False)
        self.assertEqual(r.status, WARN)
        self.assertEqual(r.hint, "charter reinit")

    def test_a_refused_profile_is_a_warning_naming_it(self):
        r = self._row(_OK + '[harness.broken]\nkind = "gemini"\ncommand = ["gemini"]\n')
        self.assertEqual(r.status, WARN)
        self.assertIn("broken", r.detail)

    def test_a_profile_named_like_a_command_is_a_warning_too(self):
        r = self._row('[harness.doctor]\nkind = "claude"\ncommand = ["claude"]\n')
        self.assertEqual(r.status, WARN)
        self.assertIn("doctor", r.detail)
        self.assertIn("charter doctor", r.hint)

    def test_a_default_naming_no_profile_is_a_warning(self):
        r = self._row('[harness]\ndefault = "nope"\n')
        self.assertEqual(r.status, WARN)
        self.assertIn("names no profile", r.detail)

    def test_a_malformed_charter_toml_costs_this_row_nothing(self):
        """The `charter.toml` row names that file. This one still reads the local file,
        because `doctor` is what somebody runs when charter is already misbehaving."""
        (config.ROOT / "charter.toml").write_text("schema = [\n")
        r = self._row(_OK + '[harness.broken]\nkind = "gemini"\ncommand = ["gemini"]\n')
        self.assertEqual(r.status, WARN)
        self.assertIn("broken", r.detail)

    def test_a_clean_file_is_ok_with_no_hint(self):
        r = self._row(_OK)
        self.assertEqual(r.status, OK)
        self.assertEqual(r.hint, "")
        self.assertIn("ok", r.detail)

    def test_each_state_names_its_own_fix(self):
        """F3. `charter reinit` adds the ignore line, which fixes one state of three: it does
        not untrack a tracked file, and it does not make git answer."""
        committable = self._row(_OK, ignored=False)
        self.assertEqual(committable.hint, "charter reinit")
        _git(config.ROOT, "add", "-f", "charter.local.toml")
        _git(config.ROOT, "commit", "-q", "-m", "tracked")
        tracked = doctor.check_harness_profiles()
        self.assertIn("git rm --cached charter.local.toml", tracked.hint)
        self.assertIn("commit", tracked.hint.split("git rm --cached charter.local.toml", 1)[1])
        dubious = "fatal: detected dubious ownership in repository at '/x'\n"
        with mock.patch("charter.util.run", return_value=_answer(128, "", dubious)):
            unknown = doctor.check_harness_profiles()
        self.assertIn("git status", unknown.hint)
        self.assertIn("dubious ownership", unknown.hint)
        self.assertEqual(len({committable.hint, tracked.hint, unknown.hint}), 3)

    def test_a_default_that_names_nothing_is_given_a_fix_not_a_consequence(self):
        """F2. Every hint names what to do."""
        r = self._row('[harness]\ndefault = "nope"\n')
        self.assertIn("[harness] default", r.hint)
        self.assertNotIn("selector", r.hint)

    def test_a_git_that_hangs_costs_one_row(self):
        """`doctor._checks` builds its rows in one list with no per-check guard, so a check
        that raised would cost every row after it."""
        (config.ROOT / "charter.local.toml").write_text(_OK)
        real = util.run

        def hung_git(cmd, *args, **kwargs):
            # Only the ignore check's own call hangs. `check_git` runs `git --version` with no
            # timeout caught (`doctor.check_git` on main at 6526928), so hanging every git
            # would take doctor down before this row ever ran — that gap is its own finding,
            # and it is not this row's to hide.
            if list(cmd[:2]) == ["git", "--no-optional-locks"]:
                raise util.ProcTimeout(cmd, 5.0)
            return real(cmd, *args, **kwargs)

        with mock.patch("charter.util.run", side_effect=hung_git):
            rows = doctor.run_all()
        self.assertEqual([r.name for r in rows], doctor.check_names())
        (row,) = [r for r in rows if r.name == "harness profiles"]
        self.assertEqual(row.status, WARN)
        self.assertIn("could not say", row.detail)

    def test_the_row_is_named_in_the_preflight(self):
        names = doctor.check_names()
        self.assertEqual(names.index("harness profiles"), names.index("charter.toml") + 1)
        self.assertIn("harness profiles", [r.name for r in doctor.run_all()])


class EverySurfaceIsRefusedTheOperatorsFile(PersonaIso):
    """The review of `a36194d` put a sentinel `charter.local.toml` at the real plane's root and
    ran with no isolation: `profiles.current()`, `harness list` and doctor all read it. Here a
    throwaway plane stands in for the real one, as far as `tests/_planeguard`'s read refusal
    is concerned. Its file is one git would commit, which is exactly when doctor's ignore
    check could answer from git alone and never open the file for the guard to see."""

    def setUp(self) -> None:
        super().setUp()
        local = config.ROOT / "charter.local.toml"
        local.write_text(_OK)
        _git(config.ROOT, "init", "-q")
        self.enterContext(mock.patch.object(
            _planeguard, "_REAL_LOCAL_PROFILES", _planeguard._both_spellings(local)))

    def test_reading_profiles_is_refused(self):
        with self.assertRaises(_planeguard.RealPlaneRead):
            profiles.current()

    def test_harness_list_is_refused(self):
        from charter import commands_harness
        with redirect_stderr(io.StringIO()), self.assertRaises(_planeguard.RealPlaneRead):
            commands_harness.cmd_harness_list(SimpleNamespace())

    def test_doctor_is_refused_even_where_git_alone_could_answer(self):
        with self.assertRaises(_planeguard.RealPlaneRead):
            doctor.check_harness_profiles()


if __name__ == "__main__":
    unittest.main()
