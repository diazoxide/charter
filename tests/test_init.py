"""`charter init` — scaffold a control plane, additively.

The write policy is the whole point: it must be safe to run in a directory that already
has a .claude/ setup, a .gitignore, and a status line the user configured themselves."""
from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

from charter import commands, instance
from tests import _envguard, _isolation


class InitIso(unittest.TestCase):
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        self.root = Path(tempfile.mkdtemp(prefix="charter-init-")).resolve()
        # Every derived setting, not just `ROOT`. `cmd_init` re-derives where the marker
        # lands (#858), so a patcher scoped to one name would put that one back and leave
        # the other nineteen pointing into this temp directory for the rest of the run.
        _isolation.point_config_at(self, self.root)

    def _init(self, **kw):
        args = SimpleNamespace(forge=kw.get("forge", "gitlab"),
                               owner=kw.get("owner", "acme"),
                               host=kw.get("host", None))
        return commands.cmd_init(args)


class TestFreshDirectory(InitIso):
    def test_creates_a_working_control_plane(self):
        self.assertEqual(self._init(), 0)
        self.assertTrue((self.root / "charter.toml").is_file())
        self.assertEqual(instance.drift(self.root), [])

    def test_the_written_config_is_valid_and_declares_the_forge(self):
        self._init(forge="github", owner="diazoxide")
        cfg = instance.load(self.root)
        self.assertEqual(cfg["schema"], instance.SCHEMA)
        self.assertEqual(cfg["forge"][0]["kind"], "github")
        self.assertEqual(cfg["forge"][0]["owner"], "diazoxide")

    def test_memory_defaults_to_local_in_the_written_config(self):
        """A fresh control plane must not publish agent notes by accident."""
        self._init()
        self.assertEqual(instance.share_of(instance.load(self.root)), "local")

    def test_gitignore_excludes_workspaces_and_the_secrets_home(self):
        self._init()
        body = (self.root / ".gitignore").read_text()
        self.assertIn("workspaces/", body)
        self.assertIn(".charter/", body)

    def test_writes_no_statusline_at_all(self):
        """#895. `init` wrote a `statusLine` key here for charter's whole life, and the
        operator asked for it to stop. Asserted on the parsed document rather than on the
        summary text, so a key written under a different spelling still fails.

        The rest of what `init` puts in this file is untouched: the plugin's `env` and the
        plane-root guard are both here, which is what makes this a removal rather than a
        file `init` stopped writing.
        """
        self._init()
        s = json.loads((self.root / ".claude" / "settings.json").read_text())
        self.assertNotIn("statusLine", s)
        self.assertIn("env", s)
        self.assertIn("hooks", s)


class TestAdditiveOnly(InitIso):
    def test_an_existing_charter_toml_is_never_overwritten(self):
        (self.root / "charter.toml").write_text('schema = 1\n# mine\n')
        self._init()
        self.assertIn("# mine", (self.root / "charter.toml").read_text())

    def test_an_existing_statusline_is_left_alone(self):
        """It always was somebody else's key; since #895 it is somebody else's key that
        charter has no writer for at all — and that is exactly why this test stays.

        The question #895 had to answer is what happens to a plane whose committed
        `.claude/settings.json` still carries the `statusLine` charter used to write. The
        answer is *nothing*: `init` and `reinit` are additive, they never delete, and a
        key charter has stopped writing is not thereby a key charter may remove from
        somebody's git-tracked file. So an existing plane's footer goes on working until
        its owner takes it out, which is their edit to make. Spelled as `my-own-thing`
        rather than `charter statusline` deliberately — charter's restraint here has never
        depended on whose command is in there, and pinning it on the foreign value keeps
        that true.
        """
        d = self.root / ".claude"
        d.mkdir()
        (d / "settings.json").write_text(json.dumps(
            {"statusLine": {"type": "command", "command": "my-own-thing"},
             "permissions": {"allow": ["Bash(ls)"]}}))
        self._init()
        s = json.loads((d / "settings.json").read_text())
        self.assertEqual(s["statusLine"]["command"], "my-own-thing")

    def test_charters_own_old_statusline_is_left_alone_too(self):
        """The other half, and the one that could have gone the other way: this is
        charter's OWN key, written by an older charter, on a plane being re-run by a newer
        one. Silently rewriting an operator's committed file is not defensible even when
        charter put the value there — so `init` leaves it, byte for byte."""
        d = self.root / ".claude"
        d.mkdir()
        mine = {"type": "command", "command": "charter statusline", "padding": 0,
                "refreshInterval": 10}
        (d / "settings.json").write_text(json.dumps({"statusLine": mine}))
        self._init()
        s = json.loads((d / "settings.json").read_text())
        self.assertEqual(s["statusLine"], mine)

    def test_unrelated_settings_keys_survive(self):
        d = self.root / ".claude"
        d.mkdir()
        (d / "settings.json").write_text(json.dumps({"permissions": {"allow": ["Bash(ls)"]}}))
        self._init()
        s = json.loads((d / "settings.json").read_text())
        self.assertEqual(s["permissions"]["allow"], ["Bash(ls)"])

    def test_existing_gitignore_lines_are_kept(self):
        (self.root / ".gitignore").write_text("node_modules/\n")
        self._init()
        self.assertIn("node_modules/", (self.root / ".gitignore").read_text())

    def test_is_idempotent(self):
        self.assertEqual(self._init(), 0)
        self.assertEqual(self._init(), 0)

    def test_malformed_existing_settings_json_does_not_crash_or_clobber(self):
        d = self.root / ".claude"
        d.mkdir()
        (d / "settings.json").write_text("{ not json")
        self._init()
        self.assertEqual((d / "settings.json").read_text(), "{ not json")


class TestSettingsFormattingPreserved(InitIso):
    """F1: adding a key to an existing settings.json must disturb the rest of the
    file as little as practical — not re-indent/reformat everything `json.dumps(...,
    indent=2)` touches. These pin the *formatting*, not just the values (the vacuous
    check `test_unrelated_settings_keys_survive` already covers values).

    **Written against `statusLine` until #895, and retargeted rather than deleted.** The
    property was never the status line's: it belongs to `_json_style`, which every writer
    of this file goes through, and the two that are left — `_ensure_guard_hook`'s `hooks`
    and `_ensure_env`'s `env` — exercise it identically. Deleting these with the key would
    have taken a live guarantee off a file charter still edits.
    """

    def test_compact_single_line_file_stays_single_line(self):
        d = self.root / ".claude"
        d.mkdir()
        original = '{"permissions":{"allow":["Bash(ls)"]},"env":{"FOO":"bar"}}'
        (d / "settings.json").write_text(original)
        self._init()
        text = (d / "settings.json").read_text()
        # A one-key addition to a compact file must not explode into multi-line output.
        self.assertNotIn("\n", text.rstrip("\n"))
        s = json.loads(text)
        self.assertEqual(s["permissions"]["allow"], ["Bash(ls)"])
        self.assertEqual(s["env"]["FOO"], "bar")
        self.assertIn("hooks", s)
        self.assertNotIn("statusLine", s)

    def test_existing_indent_width_is_matched_not_forced_to_two(self):
        d = self.root / ".claude"
        d.mkdir()
        original = ('{\n'
                    '    "permissions": {\n'
                    '        "allow": [\n'
                    '            "Bash(ls)"\n'
                    '        ]\n'
                    '    }\n'
                    '}\n')
        (d / "settings.json").write_text(original)
        self._init()
        text = (d / "settings.json").read_text()
        # The file's own 4-space indent must be reused, not overwritten with 2.
        self.assertIn('\n    "permissions"', text)
        self.assertIn('\n    "hooks"', text)
        self.assertNotIn('\n  "permissions"', text)


class TestExitCodeReflectsSkips(InitIso):
    """F2: any requested piece that could not be created must be visible in the exit
    code, not just as a warning — a malformed settings.json (the plane-root guard
    silently not written) is the same shape of failure as a blocked baseline directory
    (already non-zero), so both must return non-zero.

    It was the status line that went unwritten when this was first pinned; #895 removed
    that writer and the guard hook, which shares the file, now carries the verdict.
    """

    def test_malformed_settings_json_is_a_nonzero_exit(self):
        d = self.root / ".claude"
        d.mkdir()
        (d / "settings.json").write_text("{ not json")
        self.assertEqual(self._init(), 1)

    def test_blocked_baseline_dir_is_still_a_nonzero_exit(self):
        (self.root / "personas").write_text("not a dir")
        self.assertEqual(self._init(), 1)

    def test_fully_successful_init_is_still_zero(self):
        self.assertEqual(self._init(), 0)


class TestGitignorePresenceCheckIsPrecise(InitIso):
    """F3: the presence check for the workspaces block must key off the exact anchor
    line `workspace.set_live()` depends on, not a loose substring — a pre-existing rule
    that merely *contains* "workspaces/" (but isn't the anchor) must not suppress it."""

    def test_substring_match_without_the_anchor_does_not_suppress_it(self):
        (self.root / ".gitignore").write_text("build/workspaces/output/\n")
        self._init()
        body = (self.root / ".gitignore").read_text()
        self.assertIn("!/workspaces/.gitkeep", body)

    def test_the_real_anchor_is_still_recognised_as_present(self):
        """Nothing is re-added when every baseline rule is already there. The fixture
        lists them all deliberately — a rule missing from it would make this assert
        idempotence while actually exercising the append path."""
        (self.root / ".gitignore").write_text(
            "/workspaces/*/*\n!/workspaces/.gitkeep\n/.charter/\n"
            f"{commands.LOCAL_SETTINGS_IGNORE}\n")
        before = (self.root / ".gitignore").read_text()
        self._init()
        after = (self.root / ".gitignore").read_text()
        self.assertEqual(before, after)


class TestSummaryStaysReadable(InitIso):
    """One line naming every file charter wrote is unreadable once there are three
    harnesses to wire — it reached 254 characters, which is also what made the README's
    demo capture 40% wider and its text too small to read.

    Entries that share a file are folded into that file, so the line grows with the
    number of FILES rather than the number of things charter did to them."""

    def test_settings_entries_are_folded_into_one_mention_of_the_file(self):
        import io
        from contextlib import redirect_stderr
        buf = io.StringIO()
        with redirect_stderr(buf):
            self._init()
        # Asserted over the whole summary rather than the headline: the fold is still the
        # behaviour under test, but the paths now sit UNDER the headline rather than on it,
        # because folding took the line from 254 columns to 194 and 194 still does not
        # wrap. The fold is what keeps this one entry instead of three; the shape is what
        # keeps the line readable.
        out = buf.getvalue()
        self.assertEqual(out.count(".claude/settings.json"), 1, out)
        # Both remaining notes, named, so the fold is exercised rather than satisfied by
        # there being only one thing to fold. It was `statusLine` and `plane-root guard`
        # until #895; `env` is the other writer of this file and takes the vacated slot —
        # without a second note the count above would be trivially 1.
        self.assertIn("env", out)
        self.assertIn("plane-root guard", out)
        self.assertNotIn("statusLine", out)

    def test_the_line_stays_under_a_readable_width(self):
        import io
        from contextlib import redirect_stderr
        buf = io.StringIO()
        with redirect_stderr(buf):
            self._init()
        line = next(l for l in buf.getvalue().splitlines() if "Initialized" in l)
        # 195, and the number is accounted for rather than chosen: the line was 175
        # characters before three harnesses were wired, and charter now genuinely writes
        # one more file (`.opencode/plugin/charter.ts`, 27 characters). Folding absorbed
        # everything else — it had reached 254. A budget, not a style rule: spend it on a
        # new file, never on a longer way of naming an old one. Past this the README's
        # demo capture stops being legible at GitHub's column width.
        self.assertLess(len(line), 195, f"{len(line)} chars:\n{line}")


if __name__ == "__main__":
    unittest.main()
