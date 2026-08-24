"""The leak guard assumed a reader reads its arguments. Sometimes it writes them (#258).

`_leak_reason` fires when the program is in `_READERS` and any argument matches a vault
path. Both halves are satisfied by commands that read nothing at all:

* `cat > file <<'DOC' … DOC` — the heredoc body is stdin *data*, and `_segment_argv`
  shlex-splits the whole command string, so prose describing charter's own layout became
  `cat`'s argv and a write was denied as a read.
* `sed -i 's|<guarded-path>|…|' file`, `awk '… <guarded-path> …' f` — the path is inside
  the script operand, which is a program, not a file.
* `grep -rn "<guarded-path>" docs/` — the path is the *pattern*.

This is the second round of the same class. The first (`git commit -m "…--reveal…"`,
`grep -rn vaults .charter/vaults.json`) was fixed by shlex-accurate argv so a quoted string
stays one token; the assumption that survived it is that a program in `_READERS` opens the
tokens that follow it.

Both directions are pinned here, and the deny half comes first: this is a security guard,
and a fix that quietly widened the hole would be worse than the false positive it removes.
"""
from __future__ import annotations

import unittest

from charter import hooks


class LeakCase(unittest.TestCase):
    def denied(self, cmd: str) -> bool:
        return hooks._leak_reason(cmd) is not None


class TestItStillDeniesRealReads(LeakCase):
    """Every one of these opens a guarded path. None may become allowed."""

    def test_cat_of_a_vault_file(self):
        self.assertTrue(self.denied("cat .charter/vaults/db.json"))

    def test_head_of_the_active_persona_file(self):
        self.assertTrue(self.denied("head -n 5 .charter/active-persona"))

    def test_tail_follow(self):
        self.assertTrue(self.denied("tail -f .charter/vaults/db.json"))

    def test_grep_with_a_pattern_and_a_guarded_file(self):
        self.assertTrue(self.denied("grep -rn secret .charter/vaults/db.json"))

    def test_grep_recursive_into_the_vault_directory(self):
        """With the trailing slash — `_VAULT_PATH_RE` requires it deliberately, so that
        `.charter/vaults.json` (the registry: provider config and paths, never values) stays
        an ordinary read."""
        self.assertTrue(self.denied("grep -r . .charter/vaults/"))

    def test_grep_into_the_vault_directory_without_the_trailing_slash(self):
        """The gap this file used to record and leave open: a bare `.charter/vaults` is the
        same directory, and `grep -r` into it prints every value in it. The Read/Grep guard
        appended a slash before matching and this one did not — the two disagreeing about
        what counts as a vault, which is the failure `pretooluse_read` was written to end.
        Both go through `_vault_path_hit` now. Appending cannot make the registry a vault:
        `.charter/vaults.json/` still contains no `vaults/`."""
        self.assertTrue(self.denied("grep -r . .charter/vaults"))
        self.assertFalse(self.denied("grep -rn vaults .charter/vaults.json"))

    def test_sed_printing_a_guarded_file(self):
        self.assertTrue(self.denied("sed -n 1p .charter/active-persona"))

    def test_awk_over_a_guarded_file(self):
        self.assertTrue(self.denied("awk '{print}' .charter/browser/state.json"))

    def test_a_pattern_flag_does_not_hide_the_file(self):
        self.assertTrue(self.denied("grep -e secret .charter/vaults/db.json"))

    def test_still_denied_inside_a_compound_command(self):
        self.assertTrue(self.denied("cd /tmp && cat .charter/vaults/db.json"))

    def test_an_unparseable_command_still_fails_closed(self):
        """The invariant `test_guard_parsing` pins: not printing a secret is the one
        failure this guard may not have."""
        self.assertTrue(self.denied("cat '.charter/vaults/db.json"))


class TestAWriterIsNotAReader(LeakCase):
    def test_a_heredoc_body_mentioning_a_guarded_path(self):
        """The reported bug. The body is stdin data being written to a file under
        `workspaces/`; nothing is opened."""
        cmd = ("cat > workspaces/ws/design.md <<'DOC'\n"
               "The local active-persona file lives under .charter/active-persona.\n"
               "DOC")
        self.assertFalse(self.denied(cmd))

    def test_an_unquoted_heredoc_delimiter_too(self):
        cmd = "cat > notes.md <<DOC\nsee .charter/vaults/ for the layout\nDOC"
        self.assertFalse(self.denied(cmd))

    def test_a_dash_heredoc_too(self):
        cmd = "cat > notes.md <<-DOC\n\tsee .charter/active-persona\n\tDOC"
        self.assertFalse(self.denied(cmd))

    def test_sed_rewriting_a_mention_of_the_path(self):
        self.assertFalse(self.denied("sed -i 's|.charter/active-persona|X|' notes.md"))

    def test_awk_matching_a_mention_of_the_path(self):
        self.assertFalse(self.denied("awk '/.charter\\/active-/ {print}' notes.md"))

    def test_grep_searching_docs_for_a_mention(self):
        """Documentation about charter is exactly the text most likely to name these
        paths, and this repo is full of it."""
        self.assertFalse(self.denied('grep -rn "\\.charter/active-persona" docs/'))

    def test_grep_with_the_pattern_behind_a_flag(self):
        self.assertFalse(self.denied("grep -e .charter/active-persona docs/personas.md"))

    def test_an_interpreters_heredoc_is_not_stripped(self):
        """Only a READER's heredoc is removed. A body fed to a shell is a script, not data,
        and stripping it would hide commands from the guard.

        This assertion used to be `assertFalse`, on the reasoning that the outcome was
        unchanged either way because `bash` is not in `_READERS`. It was not unchanged: the
        body's own `cat` line is a command that runs, and until newlines became segment
        boundaries the guard could not see it — the whole heredoc collapsed into one segment
        whose program was `bash`. Stripping the body would still hide it; now that it is
        not stripped, the `cat` is read and denied, which is the point the docstring was
        always making."""
        cmd = "bash <<'EOF'\ncat .charter/vaults/db.json\nEOF"
        self.assertTrue(self.denied(cmd))

    def test_a_readers_heredoc_body_is_still_data(self):
        """The other half of the same rule, now that a newline starts a segment: a body fed
        to a *reader* is stdin data and stays stripped, so prose naming charter's layout is
        not read as a command."""
        cmd = "cat > notes.md <<'DOC'\ncat .charter/vaults/db.json\nDOC"
        self.assertFalse(self.denied(cmd))


if __name__ == "__main__":
    unittest.main()
