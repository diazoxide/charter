"""Every value a forge backend interpolates into a CLI argument must be percent-encoded.

#323: `GitHubForge.ci_status` passed `owner`, `name` and `branch` to `gh api` as bare
``-F key=value`` fields, while `open_change` — eight lines above, on the identical values —
encoded them. That asymmetry mattered because `-F` is not a string pass-through. From
``gh api --help``:

    if the value starts with `@`, the rest of the value is interpreted as a
    filename to read the value from. Pass `-` to read from standard input.

So a field value is a *filename gh dereferences*, and those values are not charter's: a
branch name comes from the tree's ``.git/HEAD`` and a repo path from ``git remote get-url
origin``. A leading ``@`` turned a status refresh into an arbitrary local file read by a
process holding the forge token.

These tests assert **the argv charter builds**, never what `gh` or `glab` do with it — the
CLIs are stubbed. That is deliberate: the invariant charter owns is "nothing external
reaches an argument the CLI gives magic meaning to", and it is checkable without a network,
a credential, or a real CLI.

**There are two correct treatments, and the context picks which.** Percent-encoding is right
for a value going into a URL path or query string, where the server decodes it again — that
is what `open_change` does. It is *wrong* for a `gh` GraphQL variable, which is a JSON
string GitHub never percent-decodes: encoding a branch there would turn ``feature/x`` into
``feature%2Fx``, match no ref, and blank the CI column for every team that puts a slash in a
branch name. The fix for a variable is the flag, not the value — ``-f/--raw-field`` has no
magic, verified against gh 2.83.2, where ``-F name=@path`` sends the file's contents and
``-f name=@path`` sends the literal string.

The table below is the point of the file. It covers every backend method that takes a value
charter did not write, so the *next* one to reach argv uninterpreted fails here rather than
shipping — which is exactly how #323 got in, one method treating its inputs and its
neighbour not.
"""

from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest import mock

from charter.forge.github import GitHubForge
from charter.forge.gitlab import GitLabForge

#: Starts with the character that makes `gh` read a file, and is otherwise inert — it names
#: nothing on any filesystem. The test asserts it never survives into argv intact, so its
#: only job is to be recognisable and to begin with ``@``.
_HOSTILE = "@sentinel-not-a-path"

#: The flags whose values `gh` gives magic meaning to: a leading ``@`` names a file to read,
#: ``@-`` names stdin. `-f/--raw-field` is deliberately NOT here — it is the escape hatch,
#: and the fix for #323 is to use it.
_MAGIC_FIELD_FLAGS = ("-F", "--field")

#: Every flag that carries a ``key=value`` payload, magic or not.
_FIELD_FLAGS = _MAGIC_FIELD_FLAGS + ("-f", "--raw-field")


def _stub_run(captured):
    """Record argv and answer with an empty JSON object.

    ``{}`` rather than ``[]``: it parses as "no result" through every caller here —
    pagination stops, `arr[0]` is never reached, `.get()` chains bottom out — so no method
    raises and none of them needs a bespoke stub.
    """
    def run(cmd, *a, **k):
        captured.append(list(cmd))
        return SimpleNamespace(stdout="{}", stderr="", returncode=0)
    return run


def _call_sites(forge, value):
    """Every backend method that receives a value charter did not write.

    ``(label, thunk)``. New forge methods taking external data belong here; that is what
    makes this file a guard rather than a snapshot of the two methods #323 happened to be
    about.
    """
    repo = {
        "id": value,
        "path_with_namespace": f"{value}/{value}",
        "default_branch": value,
    }
    return [
        ("open_change", lambda: forge.open_change(f"{value}/{value}", value)),
        ("ci_status", lambda: forge.ci_status(f"{value}/{value}", value)),
        ("repo_tree", lambda: forge.repo_tree(repo, value)),
        ("repo_tree_strict", lambda: forge.repo_tree_strict(repo, value)),
        ("list_repos", lambda: forge.list_repos(value)),
    ]


def _field_values(argv, flags=_FIELD_FLAGS):
    """The ``value`` half of every ``key=value`` element that follows one of *flags*."""
    out = []
    for flag, element in zip(argv, argv[1:]):
        if flag in flags and "=" in element:
            out.append(element.split("=", 1)[1])
    return out


def _non_field_elements(argv):
    """Everything that is not a field flag or its payload — the URL paths and positionals,
    where percent-encoding is the treatment that applies."""
    out, skip = [], False
    for i, element in enumerate(argv):
        if skip:
            skip = False
            continue
        if element in _FIELD_FLAGS:
            skip = True
            continue
        out.append(element)
    return out


class ForgeArgvEncodingTests(unittest.TestCase):
    """Run each call site against each backend with a hostile value in every slot."""

    def _argvs(self, forge, label_filter=None):
        """``{label: [argv, ...]}`` for one backend, with the CLI stubbed."""
        out = {}
        for label, thunk in _call_sites(forge, _HOSTILE):
            if label_filter and label != label_filter:
                continue
            captured = []
            with mock.patch("charter.util.run", _stub_run(captured)):
                try:
                    thunk()
                except Exception as e:      # a backend may reject the value outright —
                    pass                    # refusing to build the argv is also compliant
                    del e
            out[label] = captured
        return out

    def _forges(self):
        return (("github", GitHubForge()), ("gitlab", GitLabForge()))

    def test_no_magic_field_value_begins_with_an_at_sign(self):
        """#323's exact shape: a `-F` value starting with `@` is a file gh will open."""
        for kind, forge in self._forges():
            for label, argvs in self._argvs(forge).items():
                for argv in argvs:
                    for value in _field_values(argv, _MAGIC_FIELD_FLAGS):
                        with self.subTest(forge=kind, call=label, value=value):
                            self.assertFalse(
                                value.startswith("@"),
                                f"{kind}.{label} builds a field value gh would read as a "
                                f"filename: {value!r} in {argv!r}")

    def test_external_value_never_carried_by_a_magic_flag(self):
        """Stronger than "must not begin with `@`", and the rule that actually holds.

        `-F`'s magic is unconditional — it applies to whatever the value turns out to be,
        and charter does not get to know that in advance. So no value charter did not write
        may ride a magic flag at all, whatever it happens to start with today.
        """
        for kind, forge in self._forges():
            for label, argvs in self._argvs(forge).items():
                for argv in argvs:
                    with self.subTest(forge=kind, call=label):
                        self.assertNotIn(
                            _HOSTILE, " ".join(_field_values(argv, _MAGIC_FIELD_FLAGS)),
                            f"{kind}.{label} hands an external value to a flag that gives "
                            f"it magic meaning: {argv!r}")

    def test_external_value_percent_encoded_in_every_url_position(self):
        """The other half: anything landing in a URL path or query must be encoded.

        This is the treatment `open_change` already applies and the one #323's neighbours
        were missing — a repo id and an owner interpolated raw into an API path.
        """
        for kind, forge in self._forges():
            for label, argvs in self._argvs(forge).items():
                for argv in argvs:
                    with self.subTest(forge=kind, call=label):
                        self.assertNotIn(
                            _HOSTILE, " ".join(_non_field_elements(argv)),
                            f"{kind}.{label} interpolated an external value into a URL "
                            f"position unencoded: {argv!r}")

    def test_a_slash_in_a_branch_survives_to_the_graphql_variable(self):
        """The regression the *wrong* fix for #323 would have caused.

        Percent-encoding is right for a URL and wrong for a GraphQL variable: GitHub does
        not decode ``qualifiedName``, so an encoded ``feature/x`` matches no ref and the CI
        column silently empties — for the branch-naming convention most teams use. This
        pins the value as sent, so a future "just quote it" fails here.
        """
        forge = GitHubForge()
        captured = []
        with mock.patch("charter.util.run", _stub_run(captured)):
            forge.ci_status("acme/api", "feature/nested/branch")
        self.assertTrue(captured, "ci_status must reach the CLI")
        values = _field_values(captured[0])
        self.assertIn("feature/nested/branch", values,
                      f"the branch must reach GitHub as written: {captured[0]!r}")


if __name__ == "__main__":
    unittest.main()
