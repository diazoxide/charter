"""A commit message fed to `git commit -F -` is data to the secret-leak guard (#997).

`git commit -F - <<'MSG'` hands the heredoc body to git as the message. git stores it and runs
none of it. The leak guard used to read that body as commands, because `git` is not one of
its readers, so a message that said where charter keeps its vaults was refused as a read of
them: when the body held one apostrophe (`don't`) and named the path, the whole call stopped
lexing and the raw scan found the path; when a body line began with a reader word, that line
was a `cat` with a vault operand. The same body fed to `cat` passed. A commit message about
charter's own layout is ordinary prose in this repository, and the agent that met it had to
write the message to a file with another tool to commit at all.

The body is now data exactly as a reader's body is, and on the same terms: the heredoc is
**quoted** (an unquoted body expands, and a `$( … )` in it runs) and **no executor stands in
its pipeline** (`… | bash` runs it). Everything else keeps the body visible:

* `-e`/`--edit` in any spelling git accepts. git opens the editor on the message it just
  read, and an editor of `sh` runs that message as a script, which is measured below against
  real git rather than assumed;
* every spelling this recogniser does not read — a different subcommand (`git tag -F -`), an
  alias (`git ci -F -`), a short cluster (`-aF -`), an abbreviated `--fil=-`, a message file
  that is not stdin (`-F msg.txt`), `-F -` after `--`. Each of those is a missed allow, never
  a hidden read.

Every case runs the full `hooks.pretooluse`, the surface a Bash call reaches. The spellings
the guard now treats as data are one table, and the same table is handed to real git under
bash, zsh and dash, so the claim "git takes this body as the message" is measured for each
row the guard relies on it for.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from charter import hooks
from tests import _gitguard
from tests._isolation import PlaneIso, run_hook

VAULT = ".charter/vaults/dev.json"
READ = f"cat {VAULT}"

#: The two message shapes #997 measured refused on main 63d9406: an apostrophe that stops the
#: call lexing beside a vault path, and a body line that opens with a reader word.
APOSTROPHE = f"Don't read {VAULT} from a test\n\nThe guard refuses it now."
READER_LINE = f"Document the read guard\n\n{READ} would print it, so it is refused."
MESSAGES = {"apostrophe": APOSTROPHE, "reader line": READER_LINE}

#: Every opener the guard treats as a commit message on stdin. Shared by the guard test and
#: the real-git test, so a spelling cannot be relied on without being measured.
RECOGNISED = [
    "git commit -F -",
    "git commit -F-",
    "git commit --file=-",
    "git commit --file -",
    "git commit -a -q -F - --no-verify",
    "git commit --author 'someone else <s@example.com>' -F -",
    "git -C . commit -F -",
    "git -c user.name=someone commit --file=-",
    "git --no-pager commit -F -",
    "GIT_AUTHOR_NAME=someone git commit -F -",
    "env GIT_AUTHOR_NAME=someone git commit -F -",
]

SHELLS = {"bash": ["bash", "--norc", "--noprofile", "-c"], "zsh": ["zsh", "-f", "-c"],
          "dash": ["dash", "-c"]}


def _deny(cmd: str, cwd: str) -> bool:
    r = run_hook(hooks.pretooluse,
                 {"tool_input": {"command": cmd}, "cwd": cwd, "session_id": "s"})
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision") == "deny"


def _heredoc(opener: str, body: str, quote: str = "'", tail: str = "") -> str:
    return f"{opener} <<{quote}MSG{quote}{tail}\n{body}\nMSG"


class ACommitMessageOnStdinIsNotACommand(PlaneIso):
    """The #997 cases: refused on main, allowed now."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_the_messages_997_measured_are_allowed(self):
        for name, body in MESSAGES.items():
            cmd = _heredoc("git commit -F -", body)
            with self.subTest(message=name):
                self.assertFalse(self.denies(cmd), cmd)

    def test_every_recognised_spelling_carries_its_message_as_data(self):
        for opener in RECOGNISED:
            for name, body in MESSAGES.items():
                cmd = _heredoc(opener, body)
                with self.subTest(opener=opener, message=name):
                    self.assertFalse(self.denies(cmd), cmd)

    def test_a_double_quoted_or_dash_heredoc_is_quoted_too(self):
        """`<<"MSG"` is as inert as `<<'MSG'`, and `<<-` only strips leading tabs."""
        self.assertFalse(self.denies(_heredoc("git commit -F -", READER_LINE, quote='"')))
        cmd = f"git commit -F - <<-'MSG'\n\t{READ} would print it.\n\tMSG"
        self.assertFalse(self.denies(cmd), cmd)

    def test_a_command_chained_after_the_commit_leaves_the_message_data(self):
        """`&&` ends the commit's pipeline, so a `git push` after it neither runs the body nor
        stops it being a message."""
        cmd = _heredoc("git commit -F -", READER_LINE, tail=" && git push")
        self.assertFalse(self.denies(cmd), cmd)

    def test_no_edit_is_not_edit(self):
        """`--no-edit` is the one `--…edit` spelling that opens no editor, measured in
        :class:`RealGitAgrees`."""
        cmd = _heredoc("git commit --no-edit -F -", READER_LINE)
        self.assertFalse(self.denies(cmd), cmd)


class WhatAShellRunsStaysRefused(PlaneIso):
    """The relaxation opens nothing a shell executes. Every case here is denied on main too;
    each pins a clause of the rule."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_an_unquoted_message_expands_so_its_substitution_runs(self):
        """`<<MSG` expands before git reads it: `$(cat <vault>)` runs and its output becomes
        the message."""
        for opener in ("git commit -F -", "git commit --file=-"):
            cmd = _heredoc(opener, f"subject\n\n$(cat {VAULT})", quote="")
            with self.subTest(opener=opener):
                self.assertTrue(self.denies(cmd), cmd)

    def test_a_message_piped_into_an_executor_is_a_script(self):
        for tail in (" | bash", " | sh", " | tee out | bash", " | xargs cat"):
            cmd = _heredoc("git commit -F -", READ, tail=tail)
            with self.subTest(tail=tail):
                self.assertTrue(self.denies(cmd), cmd)

    def test_a_pipe_continued_onto_the_next_line_is_followed(self):
        cmd = f"git commit -F - <<'MSG' |\n{READ}\nMSG\nbash"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_read_after_the_terminator_is_a_command(self):
        cmd = f"git commit -F - <<'MSG'\nsubject\nMSG\n{READ}"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_read_chained_on_the_commit_line_is_a_command(self):
        cmd = _heredoc("git commit -F -", "subject", tail=f" && {READ}")
        self.assertTrue(self.denies(cmd), cmd)

    def test_an_edited_message_is_not_data(self):
        """`-e` opens the editor on the message git just read from stdin, and an editor of `sh`
        runs it (:class:`RealGitAgrees` measures that). Every spelling git accepts for it —
        the long form, an abbreviation, a short cluster — keeps the body visible."""
        for flags in ("-e", "--edit", "--edi", "--e", "-ae", "-qe"):
            for opener in (f"git commit {flags} -F -", f"git commit -F - {flags}"):
                cmd = _heredoc(opener, READER_LINE)
                with self.subTest(opener=opener):
                    self.assertTrue(self.denies(cmd), cmd)

    def test_a_redirection_target_is_not_an_option(self):
        """`> -F-` names a file. git reads no message from stdin, opens the editor, and hands
        it the heredoc as stdin, which `sh -s` runs (:class:`RealGitAgrees` measures it)."""
        for target in ("> -F-", ">--file=-", "2> -F-"):
            cmd = _heredoc(f"git -c core.editor='sh -s' commit {target}", READ)
            with self.subTest(target=target):
                self.assertTrue(self.denies(cmd), cmd)

    def test_options_end_at_a_double_dash(self):
        """After `--`, `-F -` are two pathspecs and the message is not stdin."""
        cmd = _heredoc("git commit -- -F -", READER_LINE)
        self.assertTrue(self.denies(cmd), cmd)


class TheSpellingsItDoesNotReadKeepTheBodyVisible(PlaneIso):
    """The documented misses. Each is a command whose body git also takes as data, or would
    not run, and each is still refused here because the recogniser is narrow on purpose: a
    spelling it does not read is a missed allow, never a hidden read."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_unread_spellings(self):
        for opener in ("git tag -a v1 -F -", "git ci -F -", "git commit -aF -",
                       "git commit --fil=-", "git commit -F msg.txt", "gitx commit -F -"):
            cmd = _heredoc(opener, READER_LINE)
            with self.subTest(opener=opener):
                self.assertTrue(self.denies(cmd), cmd)


class TheHandoffGateLooksPastTheMessageToo(PlaneIso):
    """A7 looks for a handoff inside `eval '…'` and `bash -c '…'` in the call with data bodies
    removed. A commit message it could not remove, holding one apostrophe, left the call
    unlexable and the look was skipped, so a handoff in a later `eval` ran with no prompt
    (`docs/hooks.md` said so). The message is removed now, so the look happens."""

    def setUp(self) -> None:
        super().setUp()
        from unittest import mock
        from charter import workspace
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"},
                                          clear=True))
        workspace.ensure("alpha")
        self.cwd = str(workspace.workspace_dir("alpha"))

    def test_a_handoff_in_a_shell_string_after_a_message_is_refused(self):
        for runner in ("eval 'charter handoff beta'", "bash -c 'charter handoff beta'"):
            cmd = f"git commit -F - <<'MSG'\nDon't hand this off\nMSG\n{runner}"
            r = run_hook(hooks.pretooluse,
                         {"tool_input": {"command": cmd}, "cwd": self.cwd, "session_id": "s"})
            out = (r or {}).get("hookSpecificOutput") or {}
            with self.subTest(runner=runner):
                self.assertEqual("deny", out.get("permissionDecision"), cmd)
                self.assertIn("inside a string or a heredoc a shell runs",
                              out.get("permissionDecisionReason", ""))


class RealGitAgrees(unittest.TestCase):
    """The source of truth is git under a real shell, not charter's reading of either."""

    def setUp(self) -> None:
        if not shutil.which("git"):
            self.skipTest("git is not installed")
        self.repo = Path(tempfile.mkdtemp(prefix="charter-997-"))
        self.addCleanup(shutil.rmtree, self.repo, True)
        self.env = {k: v for k, v in os.environ.items()
                    if k not in ("GIT_EDITOR", "EDITOR", "VISUAL", "GIT_DIR", "GIT_WORK_TREE")}
        self.env.update(_gitguard.environment())
        self.env.update(GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@example.com",
                        GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@example.com")
        self._git("init", "-q", "-b", "main", ".")

    def _git(self, *argv: str) -> str:
        return subprocess.run(["git", *argv], cwd=self.repo, env=self.env, check=True,
                              capture_output=True, text=True, timeout=30).stdout

    def _shell(self, argv: list[str], script: str) -> subprocess.CompletedProcess:
        return subprocess.run(argv + [script], cwd=self.repo, env=self.env,
                              capture_output=True, text=True, timeout=30)

    def test_every_recognised_spelling_makes_the_body_the_message(self):
        measured = 0
        for shell, argv in SHELLS.items():
            if not shutil.which(argv[0]):
                continue
            for opener in RECOGNISED:
                body = f"{shell}: {opener}\n\n{READ} would print it."
                script = _heredoc(f"{opener} --allow-empty", body)
                with self.subTest(shell=shell, opener=opener):
                    done = self._shell(argv, script)
                    self.assertEqual(0, done.returncode, done.stderr)
                    self.assertEqual(body, self._git("log", "-1", "--format=%B").strip())
                    measured += 1
        self.assertGreater(measured, 0, "no shell ran — nothing was compared")

    def test_an_sh_editor_runs_the_message_only_when_edit_is_asked_for(self):
        """Why `-e` keeps the body visible. With `core.editor=sh`, `git commit -e -F -` reads
        the message from stdin and then hands the message FILE to `sh`, which runs it. Without
        `-e`, and with `--no-edit`, no editor opens and nothing in the body runs."""
        ran = self.repo / "ran"
        for flags, runs in (("-e", True), ("--edit", True), ("--edi", True), ("-ae", True),
                            ("", False), ("--no-edit", False)):
            if ran.exists():
                ran.unlink()
            opener = f"git -c core.editor=sh commit --allow-empty {flags} -F -"
            script = _heredoc(opener, f"touch {ran}")
            with self.subTest(flags=flags):
                done = self._shell(SHELLS["bash"], script)
                self.assertEqual(0, done.returncode, done.stderr)
                self.assertEqual(runs, ran.exists())

    def test_a_redirection_target_spelled_like_the_flag_leaves_stdin_to_the_editor(self):
        """Why redirections come out before the options are read. `> -F-` is a file; git
        reads no message, opens the editor, and `sh -s` runs the heredoc it inherits."""
        ran = self.repo / "ran"
        script = _heredoc("git -c core.editor='sh -s' commit --allow-empty > -F-",
                          f"touch {ran}")
        self._shell(SHELLS["bash"], script)
        self.assertTrue((self.repo / "-F-").exists(), "the target is a file")
        self.assertTrue(ran.exists(), "the editor ran the heredoc")


if __name__ == "__main__":
    unittest.main()
