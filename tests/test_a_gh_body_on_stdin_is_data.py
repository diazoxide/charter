"""A pull request or issue body fed to `gh … --body-file -` is data to the secret-leak guard (#1070).

`gh pr create --body-file - <<'EOF' | tail -1` hands the heredoc body to gh as the PR body. gh
posts it and runs none of it. The leak guard kept that body visible, because `gh` is not one of
its readers, and read it as commands. One apostrophe in the prose (`program's`) stopped the call
lexing. On that path argv is a whitespace split that does not keep `\\n` as a boundary, so every
word of the body became an operand of `tail -1`. `_spliced_operands` then joined the adjacent
words `.` (from "`~`.") and `Charter` into `.Charter`, which `_VAULT_PATH_RE` matches with its
case folding, and the call was refused as a read of a vault. A body naming the vault path
beside an apostrophe was refused by the raw scan the same way. The same body fed to `cat` passed.

This is the #997 rule for `git commit -F -` applied to gh, on the same terms: the heredoc is
**quoted** and **no executor stands in its pipeline**. The spellings read are `-F -`, `-F-`,
`--body-file -` and `--body-file=-` after a literal `gh pr|issue create|comment`. The body stays
visible for everything else:

* `-e`/`--editor` in any spelling, and a short cluster holding `e`. An editor is handed the body
  as a FILE, and an editor of `sh` runs it. gh refuses to open one when the body is on stdin:
  measured on gh 2.83.2 with a dummy token, an empty config dir and an editor that leaves a
  marker, `pr create` and `issue create` with `-F -` exit with "--editor or enabled
  prefer_editor_prompt configuration are not supported in non-tty mode", with `-e` or with
  `prefer_editor_prompt` enabled, with or without `GH_FORCE_TTY=1`, and neither the editor nor
  the body ran. `pr comment` refuses `-e` beside `-F` outright. That is measured by hand rather
  than here, because this suite refuses to spawn `gh` (`tests/_planeguard.py`). Refusing the
  spelling as well costs a missed allow;
* a different subcommand (`gh pr edit`, `gh release create -F -`, `gh api --input -`), a body file
  that is not stdin, `-F -` after `--`, and a redirection target spelled like the flag. Each is a
  missed allow, never a hidden read.
"""

from __future__ import annotations

import unittest

from charter import hooks
from tests._isolation import PlaneIso, run_hook

VAULT = ".charter/vaults/dev.json"
READ = f"cat {VAULT}"

#: The #1070 shape reduced to what the refusal needed: prose whose apostrophe stops the call
#: lexing, and a sentence ending in `.` before one starting with `Charter`.
SPLICED = "It keeps the program's `~/`. Charter expands it."
#: The raw-scan sibling: an apostrophe beside the vault path.
APOSTROPHE = f"Don't read {VAULT} from a test."
#: A body line that opens with a reader word.
READER_LINE = f"Document the read guard\n\n{READ} would print it, so it is refused."
BODIES = {"spliced": SPLICED, "apostrophe": APOSTROPHE, "reader line": READER_LINE}

RECOGNISED = [
    "gh pr create --base main --title t --body-file -",
    "gh pr create --title t --body-file=-",
    "gh pr create --title t -F -",
    "gh pr create --title t -F-",
    # An ESCAPED backtick starts no substitution, so its line is attributed (#1070's title).
    "gh pr create --title \"it's \\`~/\\`\" -F -",
    "gh pr create --title keeps\\`~\\` -F -",
    "gh pr comment 12 --body-file -",
    "gh issue create -R o/r --title t -F -",
    "gh issue comment 5 -F -",
    "env GH_REPO=o/r gh pr create --title t -F -",
]


def _deny(cmd: str, cwd: str) -> bool:
    r = run_hook(hooks.pretooluse,
                 {"tool_input": {"command": cmd}, "cwd": cwd, "session_id": "s"})
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision") == "deny"


def _heredoc(opener: str, body: str, quote: str = "'", tail: str = "") -> str:
    return f"{opener} <<{quote}EOF{quote}{tail}\n{body}\nEOF"


class AGhBodyOnStdinIsNotACommand(PlaneIso):
    """The #1070 cases: refused on main, allowed now."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_the_reduced_1070_command_is_allowed(self):
        cmd = _heredoc("gh pr create --title t --body-file -", SPLICED, tail=" | tail -1")
        self.assertFalse(self.denies(cmd), cmd)

    def test_the_1070_command_reads_no_vault(self):
        """The refused call's shape, verbatim in structure: a commit message and a PR body, each
        on stdin, with a push and two `tail -1`s between them."""
        cmd = ("git add a.py && git commit -q -F - <<'EOF'\n"
               "A listed profile command keeps its program's `~/`\n\n"
               "names a directory called `~`. Charter expands the program's leading `~`.\n"
               "EOF\n"
               "git push -q -u origin HEAD 2>&1 | grep -v \"^remote:\" | tail -1; "
               "gh pr create --base main --title \"A listed command keeps its program's "
               "\\`~/\\`\" --body-file - <<'EOF' | tail -1\n"
               "## What was wrong\n\n"
               "a directory literally named `~`. Charter expands the program's leading `~` "
               "itself (`profiles.expanded_command`).\n"
               "EOF")
        self.assertIsNone(hooks._leak_reason(cmd, str(self.tmp)), cmd)

    def test_every_recognised_spelling_carries_its_body_as_data(self):
        for opener in RECOGNISED:
            for name, body in BODIES.items():
                cmd = _heredoc(opener, body, tail=" | tail -1")
                with self.subTest(opener=opener, body=name):
                    self.assertFalse(self.denies(cmd), cmd)

    def test_a_double_quoted_heredoc_is_quoted_too(self):
        cmd = _heredoc("gh pr create --title t -F -", READER_LINE, quote='"')
        self.assertFalse(self.denies(cmd), cmd)


class WhatAShellRunsStaysRefused(PlaneIso):
    """The relaxation opens nothing a shell executes. Every case here is denied on main too."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_an_unquoted_body_expands_so_its_substitution_runs(self):
        cmd = _heredoc("gh pr create --title t -F -", f"body\n\n$(cat {VAULT})", quote="")
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_body_piped_into_an_executor_is_a_script(self):
        for tail in (" | bash", " | sh", " | tail -1 | bash", " | xargs cat"):
            cmd = _heredoc("gh pr create --title t -F -", READ, tail=tail)
            with self.subTest(tail=tail):
                self.assertTrue(self.denies(cmd), cmd)

    def test_a_read_after_the_terminator_is_a_command(self):
        cmd = f"gh pr create --title t -F - <<'EOF'\nbody\nEOF\n{READ}"
        self.assertTrue(self.denies(cmd), cmd)

    def test_a_read_chained_on_the_opener_line_is_a_command(self):
        for tail in (f" && {READ}", f" | tail -1; {READ}", f" | {READ}"):
            cmd = _heredoc("gh pr create --title t -F -", "body", tail=tail)
            with self.subTest(tail=tail):
                self.assertTrue(self.denies(cmd), cmd)

    def test_an_editor_keeps_the_body_visible(self):
        for flags in ("-e", "--editor", "--editor=true", "-de", "-e=true"):
            for opener in (f"gh pr create {flags} -F -", f"gh issue comment 5 -F - {flags}"):
                cmd = _heredoc(opener, READER_LINE)
                with self.subTest(opener=opener):
                    self.assertTrue(self.denies(cmd), cmd)

    def test_a_redirection_target_is_not_an_option(self):
        for target in ("> -F-", ">--body-file=-", "2> -F-"):
            cmd = _heredoc(f"gh pr create --title t {target}", READ)
            with self.subTest(target=target):
                self.assertTrue(self.denies(cmd), cmd)

    def test_a_live_backtick_on_the_opener_line_keeps_the_body_visible(self):
        """An unescaped backtick is a substitution, which can re-pipe the body into a shell, so
        the line stays unattributable (review round 5, ruling C). `\\\\`` is an escaped backslash
        before a LIVE backtick, and counts as one."""
        for title in ('"it\'s `~/`"', '"a \\\\`bash\\\\`"', "`bash`"):
            cmd = _heredoc(f"gh pr create --title {title} -F -", READER_LINE)
            with self.subTest(title=title):
                self.assertTrue(self.denies(cmd), cmd)

    def test_options_end_at_a_double_dash(self):
        cmd = _heredoc("gh pr comment 12 -- -F -", READER_LINE)
        self.assertTrue(self.denies(cmd), cmd)


class TheSpellingsItDoesNotReadKeepTheBodyVisible(PlaneIso):
    """Narrow on purpose: a spelling it does not read is a missed allow, never a hidden read."""

    def denies(self, cmd: str) -> bool:
        return _deny(cmd, str(self.tmp))

    def test_unread_spellings(self):
        for opener in ("gh pr edit 3 -F -", "gh release create v1 -F -", "gh api x --input -",
                       "gh pr create -F body.md", "gh -F -", "ghx pr create -F -",
                       "gh pr create -dF -"):
            cmd = _heredoc(opener, READER_LINE)
            with self.subTest(opener=opener):
                self.assertTrue(self.denies(cmd), cmd)


if __name__ == "__main__":
    unittest.main()
