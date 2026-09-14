"""`gh … -F <vault>` reads the vault and uploads it to the forge, so the guard denies it (#1086 class 5).

`gh` is not a printer, so it is absent from `hooks._READERS`, and the file its body, notes or
template flag names was never asked of `_names_a_vault_path`. `gh pr create -F .charter/vaults/x.json`
reads the vault and publishes it as a pull request body — worse than printing it, because the
value lands on the forge. This is a plain argv-reader case: the flag's value is an opened path,
so it now goes through the same `_opens` a reader operand does.

Stdin stays data, which #1070 established: `-F -` (and `gh api`'s `@-`) is the heredoc body the
guard reads as text, not a path. An ordinary non-vault body file (`-F /tmp/body.md`) stays allowed
too — the deny is decided by `_names_a_vault_path`, not by the flag.

The flags and `gh api`'s file forms were read from `gh --help` text on gh 2.83.2; no live `gh` is
run here, and none is needed — the guard decides on the argv, not on gh's behaviour.

Every line of the parse is pinned by a case that goes red without it. The deletion sweep on
#1087's first head found twenty lines it could not tell from their absence, and ran out of
time on `_leak_reason`'s own `"gh"`: each is pinned below, or is gone (the commit says
which, and why). `TheOperandsGhOpens` pins the parse without a plane, for the lines whose
only observable difference is a `None` or a `-` that the vault check would have swallowed.
"""

from __future__ import annotations

import unittest

from charter import hooks
from tests._isolation import PlaneIso, run_hook

VAULT = ".charter/vaults/dev.json"
#: A vault whose own name holds an `=`. The file after `--flag=` (and after gh's `key=@`) is
#: everything after the FIRST `=`, and only a name with a second `=` in it can tell that
#: apart from "everything after the last one".
VAULT_EQ = ".charter/vaults/a=b.json"


def _deny_reason(cmd: str, cwd: str) -> str | None:
    r = run_hook(hooks.pretooluse,
                 {"tool_input": {"command": cmd}, "cwd": cwd, "session_id": "s"})
    out = (r or {}).get("hookSpecificOutput", {}) or {}
    return out.get("permissionDecisionReason") if out.get("permissionDecision") == "deny" else None


class AGhBodyFileNamingAVaultIsDenied(PlaneIso):
    """Each was allowed on main; each is denied now, and names the vault-read rule."""

    #: One spelling per way gh names the file: the two body-file forms and their `=`/attached
    #: variants, notes and template, and gh api's two file routes. `-F` is `--body-file` for the
    #: body subcommands and `--field` (with `@`) for `gh api`.
    DENIED = [
        "gh pr create -F {v}",
        "gh pr create --body-file {v}",
        "gh pr create --body-file={v}",
        "gh pr create -F{v}",
        "gh issue create -F {v}",
        "gh issue comment 5 --body-file {v}",
        "gh pr comment 5 -F {v}",
        "gh pr edit 7 --body-file {v}",
        "gh pr review 7 -F {v}",
        "gh pr merge 7 -F {v}",
        "gh release create v1 --notes-file {v}",
        "gh release edit v1 -F {v}",
        "gh pr create -T {v}",
        "gh pr create -T{v}",
        "gh issue create -T{v}",
        "gh pr create --template={v}",
        "gh api repos/o/r/pulls --input {v}",
        "gh api repos/o/r/pulls --input={v}",
        "gh api x -F body=@{v}",
        "gh api x --field body=@{v}",
        "gh api x --field=body=@{v}",
        "gh api x -Fbody=@{v}",
        # behind a wrapper, and after a cd that puts the relative path over the vault
        "env gh pr create -F {v}",
        "nohup gh pr create -F {v}",
        "./gh pr create -F {v}",
        # gh by its basename: a full path, and the case macOS's filesystem does not tell apart
        "/usr/bin/gh pr create -F {v}",
        "GH pr create -F {v}",
    ]

    def test_the_minimal_case_is_denied_and_names_the_rule(self):
        reason = _deny_reason(f"gh pr create -F {VAULT}", str(self.tmp))
        self.assertIsNotNone(reason)
        self.assertIn("reads a vault/secret file directly", reason)

    def test_a_cd_puts_a_relative_body_file_over_the_vault(self):
        reason = _deny_reason("cd .charter && gh pr create -F vaults/dev.json", str(self.tmp))
        self.assertIsNotNone(reason)

    def test_every_spelling_that_reads_the_file_is_denied(self):
        for tpl in self.DENIED:
            cmd = tpl.format(v=VAULT)
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(_deny_reason(cmd, str(self.tmp)), cmd)
                # ..and it is the vault-read rule, not some other guard catching it.
                self.assertEqual(hooks._READ_REASON, hooks._leak_reason(cmd, str(self.tmp)), cmd)

    def test_the_file_is_everything_after_the_first_equals(self):
        """`--flag=value` splits at its first `=`, and so does gh's `key=@path` field (gh's
        `parseField` takes the index of the FIRST `=`), so a file whose own name holds one
        is still that file. This is the one shape where splitting from the other end would
        hand back `b.json` — a name that is nobody's vault."""
        for tpl in ("gh pr create --body-file={v}",
                    "gh release create v1 --notes-file={v}",
                    "gh api x --input={v}",
                    "gh api x -F body=@{v}",
                    "gh api x --field=body=@{v}",
                    "gh api x -Fbody=@{v}"):
            cmd = tpl.format(v=VAULT_EQ)
            with self.subTest(cmd=cmd):
                self.assertEqual(hooks._READ_REASON, hooks._leak_reason(cmd, str(self.tmp)), cmd)

    def test_a_flag_before_the_subcommand_does_not_hide_api(self):
        """The subcommand is the first word that is not a flag, so a flag in front of `api`
        does not turn its `--input`/`--field` into words the guard has no grammar for. gh's
        root takes no flag today (`--help` and `--version` each run nothing, gh 2.83.2), so
        no command gh runs is told apart by this — the guard reads past the word rather than
        stopping at it because its miss here would be a missed deny, and a root flag gh grows
        later would then hide a read. Only `api`'s two file routes can tell: a body
        subcommand's `-F` reads the next word whichever way the subcommand was found."""
        for cmd in (f"gh --version api x --input {VAULT}",
                    f"gh --version api x --field body=@{VAULT}"):
            with self.subTest(cmd=cmd):
                self.assertEqual(hooks._READ_REASON, hooks._leak_reason(cmd, str(self.tmp)), cmd)


class WhatGhDoesNotOpenStaysAllowed(PlaneIso):
    """The deny is decided by the path, not by naming gh. Each of these reads no vault, and
    every one is allowed on main too — so denying it would be a regression the differential
    forbids."""

    def allowed(self, cmd: str) -> bool:
        return _deny_reason(cmd, str(self.tmp)) is None

    def test_stdin_is_data_not_a_path(self):
        for cmd in (f"gh pr create -F -",
                    f"gh pr create --body-file=-",
                    f"gh api x --input -",
                    f"gh api x -F body=@-",
                    f"gh pr create -F - <<'EOF'\nnotes about {VAULT}\nEOF"):
            with self.subTest(cmd=cmd):
                self.assertTrue(self.allowed(cmd), cmd)

    def test_stdin_is_data_even_inside_the_vault_directory(self):
        """A relative operand resolves against where the shell stands — after
        `cd .charter/vaults`, a body file `dev.json` IS the vault, and is denied. `-` is not
        an operand of that kind: it is stdin wherever the shell stands, never
        `.charter/vaults/-`. gh api's `@-` is the same `-`, and so is a gist read from
        stdin."""
        self.assertFalse(self.allowed("cd .charter/vaults && gh pr create -F dev.json"))
        for cmd in ("cd .charter/vaults && gh pr create -F -",
                    "cd .charter/vaults && gh pr create -F-",
                    "cd .charter/vaults && gh pr create --body-file=-",
                    "cd .charter/vaults && gh api x --input -",
                    "cd .charter/vaults && gh api x --input=-",
                    "cd .charter/vaults && gh api x -F body=@-",
                    "cd .charter/vaults && gh api x -Fbody=@-",
                    "cd .charter/vaults && gh gist create -"):
            with self.subTest(cmd=cmd):
                self.assertTrue(self.allowed(cmd), cmd)

    def test_a_flag_with_nothing_after_it_names_no_file(self):
        """gh answers `flag needs an argument`. The guard reads no file and raises
        nothing — a value is taken from the next word only while there is one."""
        for cmd in ("gh pr create -F",
                    "gh pr create --body-file",
                    "gh pr create -T",
                    "gh release create v1 --notes-file",
                    "gh api x --input",
                    "gh api x -F",
                    "gh api x --field"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(hooks._leak_reason(cmd, str(self.tmp)), cmd)
                self.assertEqual([], hooks._gh_file_operands(cmd.split()), cmd)

    def test_the_at_sign_is_what_makes_a_field_a_file(self):
        """`-F body=@<path>` opens the path; `-F body=<path>` sends those characters as
        the value and opens nothing. gh splits the field at its FIRST `=`, so in
        `body=x=@<path>` the value is the literal `x=@<path>` — no `@` in front, no file.
        A field that is nothing but a path spelling is text to gh, the way a vault path
        inside a heredoc body is text (#1070)."""
        for cmd in (f"gh api x -F body={VAULT}",
                    f"gh api x --field body={VAULT}",
                    f"gh api x --field=body={VAULT}",
                    f"gh api x -Fbody={VAULT}",
                    f"gh api x -F body=x=@{VAULT}"):
            with self.subTest(cmd=cmd):
                self.assertTrue(self.allowed(cmd), cmd)

    def test_the_body_text_is_not_the_body_file(self):
        """`-b`/`--body` is the text itself; only `--body-file` names a file, and
        `--body=` is a different flag from `--body-file=`, not a prefix of it."""
        for cmd in (f"gh pr create --title t --body {VAULT}",
                    f"gh pr create --title t --body={VAULT}",
                    f"gh pr create --title t -b {VAULT}"):
            with self.subTest(cmd=cmd):
                self.assertTrue(self.allowed(cmd), cmd)

    def test_an_ordinary_body_file_is_untouched(self):
        for cmd in ("gh pr create --title t --body-file /tmp/body.md",
                    "gh pr create -F README.md",
                    "gh release create v1 --notes-file notes.md"):
            with self.subTest(cmd=cmd):
                self.assertTrue(self.allowed(cmd), cmd)

    def test_raw_field_is_a_literal_string_not_a_file(self):
        """`gh api -f/--raw-field key=@path` is a literal `@path` string; gh reads no file, so
        the guard must not either (measured from `gh api --help`)."""
        for cmd in (f"gh api x -f body=@{VAULT}",
                    f"gh api x --raw-field body=@{VAULT}"):
            with self.subTest(cmd=cmd):
                self.assertTrue(self.allowed(cmd), cmd)

    def test_options_end_at_a_double_dash(self):
        self.assertTrue(self.allowed(f"gh pr create -F - -- -F {VAULT}"))

    def test_only_gh_is_read_as_gh(self):
        """The program is matched by its basename, once, in `_leak_reason`. A program that is
        not gh has no body-file grammar the guard knows, so its `-F <vault>` is a flag the
        guard cannot read — a missed deny, never a wrong one — while `/usr/bin/gh` and `GH`
        (one file to macOS's filesystem) are gh and are denied above."""
        for cmd in (f"ghx pr create -F {VAULT}",
                    f"xgh pr create -F {VAULT}",
                    f"ghx api x --input {VAULT}"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(hooks._leak_reason(cmd, str(self.tmp)), cmd)


class TheOperandsGhOpens(unittest.TestCase):
    """The parse on its own, without a plane: which words `_gh_file_operands` hands to the
    vault check, and what `_gh_at_path` makes of a field."""

    def test_a_field_is_a_file_only_behind_an_at_sign_after_the_first_equals(self):
        for value, path in (("body=@notes.md", "notes.md"),
                            ("body=notes.md", None),
                            ("body=@a=b.md", "a=b.md"),
                            ("body=x=@notes.md", None),
                            ("body=@-", "-"),
                            ("body=", None),
                            ("@notes.md", None),
                            ("", None)):
            with self.subTest(value=value):
                self.assertEqual(path, hooks._gh_at_path(value))

    def test_the_paths_a_gh_command_opens(self):
        for cmd, paths in ((f"gh pr create -F {VAULT}", [VAULT]),
                           (f"gh pr create -F{VAULT}", [VAULT]),
                           (f"gh pr create -T{VAULT}", [VAULT]),
                           (f"gh pr create --body-file={VAULT_EQ}", [VAULT_EQ]),
                           (f"gh api x --input {VAULT}", [VAULT]),
                           (f"gh api x -F body=@{VAULT}", [VAULT]),
                           (f"gh api x -F body=@{VAULT_EQ}", [VAULT_EQ]),
                           (f"gh api x -F body={VAULT}", []),
                           (f"gh api x -f body=@{VAULT}", []),
                           ("gh pr create -F -", []),
                           ("gh pr create -F- --title t", []),
                           ("gh api x --input -", []),
                           ("gh api x -F body=@-", []),
                           ("gh pr create -F", []),
                           (f"gh pr create -F - -- -F {VAULT}", []),
                           (f"gh pr create --body={VAULT}", []),
                           (f"gh pr create -F {VAULT} -T {VAULT_EQ}", [VAULT, VAULT_EQ])):
            with self.subTest(cmd=cmd):
                self.assertEqual(paths, hooks._gh_file_operands(cmd.split()))


if __name__ == "__main__":
    unittest.main()
