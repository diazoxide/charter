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
"""

from __future__ import annotations

from charter import hooks
from tests._isolation import PlaneIso, run_hook

VAULT = ".charter/vaults/dev.json"


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


if __name__ == "__main__":
    import unittest
    unittest.main()
