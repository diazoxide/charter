"""#326 — the open-change identifier is the one forge field passed through as-is.

`ci` is pinned to seven literals by each backend's `_CI_MAP`, and `sigil` is a class
constant, so both are safe by construction. `change` was whatever the forge's JSON
`number` (GitHub) or `iid` (GitLab) field held: `state_for_repo` stored it, the cache
kept it, `read_for` returned it, and `statusline` interpolated it into the rendered row.

`base.Forge.open_change` is annotated ``int | None`` and both implementations `.get()` a
field documented as a number, so the protocol already says what this is. These tests
hold the boundary to it — at BOTH ends, because a cache entry survives an upgrade by two
hours (`DISPLAY_TTL`) and a hand-edited or already-poisoned cache never passes through
`state_for_repo` at all.
"""

from __future__ import annotations

import json
import time
import unittest
from pathlib import Path
from unittest import mock

from charter import glstate

from tests._isolation import PersonaIso

#: The demonstration in #326: a change "number" that is an escape sequence.
ERASE_DISPLAY = "\x1b[2J"
OSC_TITLE = "\x1b]0;pwned\x07"


class _Forge:
    """A forge whose open_change returns whatever it was handed — the shim `gh` from
    #326, without the subprocess."""

    kind, host, cli, change_sigil = "github", "github.com", "gh", "#"

    def __init__(self, change):
        self.change = change
        self.calls = []

    def open_change(self, path, branch):
        self.calls.append((path, branch))
        return self.change

    def ci_status(self, path, branch):
        return "success"


def _state_for(change):
    """Drive `state_for_repo` with a forge returning *change*. Returns (state, forge)
    so a test can assert the call actually happened before trusting the result."""
    forge = _Forge(change)
    with mock.patch("charter.forge.registry.resolve_host", return_value=forge), \
         mock.patch("charter.glstate._remote_url",
                    return_value="https://github.com/acme/api.git"), \
         mock.patch("charter.glstate._remote_path", return_value="acme/api"):
        return glstate.state_for_repo(Path("/tmp/acme-api"), "main"), forge


class ChangeIsWhatTheProtocolSaysItIs(unittest.TestCase):
    def test_an_escape_sequence_never_becomes_a_change_number(self):
        state, forge = _state_for(f"{ERASE_DISPLAY}31")
        # Precondition: the hostile value really did cross the forge boundary. Without
        # this the assertion below passes just as well when nothing was ever called.
        self.assertEqual(forge.calls, [("acme/api", "main")],
                         "open_change was never reached — the test proves nothing")
        self.assertIsNone(state["change"])

    def test_an_osc_string_never_becomes_a_change_number(self):
        state, forge = _state_for(OSC_TITLE)
        self.assertEqual(forge.calls, [("acme/api", "main")])
        self.assertIsNone(state["change"])

    def test_a_dict_never_becomes_a_change_number(self):
        """`arr[0].get("number")` returns whatever the JSON held — an object, a list and
        a null are all things a forge response can put there."""
        state, _ = _state_for({"number": 1})
        self.assertIsNone(state["change"])

    def test_a_genuine_number_is_kept(self):
        state, forge = _state_for(42)
        self.assertEqual(forge.calls, [("acme/api", "main")])
        self.assertEqual(state["change"], 42)
        self.assertEqual(state["ci"], "success")
        self.assertEqual(state["sigil"], "#")

    def test_a_numeric_string_is_coerced_rather_than_dropped(self):
        """A forge that serialises its id as a string is answering the question; only
        the type is off. Dropping it would blank a real PR number over a JSON detail."""
        self.assertEqual(_state_for("42")[0]["change"], 42)

    def test_no_open_change_stays_none(self):
        self.assertIsNone(_state_for(None)[0]["change"])

    def test_a_nonsense_number_is_dropped(self):
        """Zero and negatives are not change identifiers on any forge charter speaks to,
        and a value that is *shaped* like a number but cannot be one is the same defect
        as a value that is not a number at all."""
        for bad in (0, -7, "-7"):
            self.assertIsNone(_state_for(bad)[0]["change"], bad)


class AnAlreadyPoisonedCacheDoesNotRender(PersonaIso):
    """`state_for_repo` guards what is written from now on. `read_for` guards what is
    already on disk — an entry written by the charter that had the bug renders for two
    hours after the upgrade, and the cache file is an ordinary writable JSON file that
    nothing signs."""

    def _write(self, entry: dict) -> Path:
        d = self.tmp / "poisoned"
        d.mkdir(parents=True, exist_ok=True)
        f = glstate._cache_file()
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(json.dumps({str(d): {"ts": time.time(), "branch": "main", **entry}}))
        return d

    def test_an_escape_in_a_cached_change_is_dropped_on_read(self):
        d = self._write({"change": f"{ERASE_DISPLAY}31", "ci": "success"})
        got = glstate.read_for([d], {d: "main"})
        # Precondition: the entry was fresh and branch-matching, so it really was read.
        self.assertIn(d, got, "the cache entry was skipped — the test proves nothing")
        self.assertIsNone(got[d]["change"])
        self.assertEqual(got[d]["ci"], "success")

    def test_an_escape_in_the_legacy_mr_key_is_dropped_too(self):
        d = self._write({"mr": OSC_TITLE, "ci": None})
        got = glstate.read_for([d], {d: "main"})
        self.assertIn(d, got)
        self.assertIsNone(got[d]["change"])

    def test_a_good_cached_change_still_renders(self):
        d = self._write({"change": 9, "ci": "running", "sigil": "#"})
        got = glstate.read_for([d], {d: "main"})
        self.assertEqual(got[d], {"change": 9, "ci": "running", "sigil": "#"})


if __name__ == "__main__":
    unittest.main()
