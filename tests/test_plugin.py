"""The Claude Code plugin, and the one place a hook is allowed to shout.

Hooks swallow exceptions by design — a tally must never break a turn. Version skew is the
exception: a stale CLI would silently stop firing the gate while everything still looked
installed, which is the failure this guard exists to prevent."""
from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

from charter import __version__, hooks
from tests import _envguard

ROOT = Path(__file__).resolve().parents[1]


def _hooks_json() -> dict:
    return json.loads((ROOT / "hooks" / "hooks.json").read_text())["hooks"]


def _flat_hooks() -> list[tuple[str, dict]]:
    """(event, hook-dict) for every individual hook command declared in hooks.json —
    flattened across every matcher group of every event."""
    h = _hooks_json()
    return [(event, hook) for event, entries in h.items()
            for entry in entries for hook in entry["hooks"]]


class TestPluginManifest(unittest.TestCase):
    def test_manifest_exists_and_names_the_plugin(self):
        m = json.loads((ROOT / ".claude-plugin" / "plugin.json").read_text())
        self.assertEqual(m["name"], "charter")
        self.assertTrue(m.get("description"))

    def test_hooks_json_declares_every_event_the_engine_implements(self):
        h = _hooks_json()
        for event in ("SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse"):
            self.assertIn(event, h, event)

    def test_every_hook_command_invokes_the_installed_cli(self):
        """Hooks must call the CLI on PATH, not a path inside the plugin or the user's
        control plane — the plugin ships no Python."""
        cmds = [hook["command"] for _, hook in _flat_hooks()]
        self.assertTrue(cmds)
        for c in cmds:
            self.assertIn("charter", c)

    def test_hook_engine_commands_carry_plugin_version(self):
        """Only the ``charter hook <name>`` dispatch path is skew-checked (see
        hooks.skew_message) — the other five commands (workspace/persona/doctor/
        gl-refresh) are raw CLI subcommands, not routed through that engine, and their
        subparsers don't accept --plugin-version at all, so tagging it on would make the
        hook fail outright."""
        engine_cmds = [hook["command"] for _, hook in _flat_hooks()
                       if "charter hook " in hook["command"]]
        self.assertTrue(engine_cmds, "manifest declares no engine hooks — did they move?")
        for c in engine_cmds:
            self.assertIn("--plugin-version", c)
        # Tied to the handler registry rather than a magic count: a hardcoded number
        # only says "something changed", and has to be bumped by hand every time a
        # handler is legitimately added. This says the manifest and the engine agree.
        from charter import hooks as _h
        declared = {c.split("charter hook ")[1].split()[0] for c in engine_cmds}
        self.assertTrue(declared <= set(_h._HANDLERS),
                        f"manifest names handlers the engine lacks: "
                        f"{sorted(declared - set(_h._HANDLERS))}")

    def test_sessionstart_carries_no_matcher(self):
        """An absent matcher matches startup|resume|clear|compact|fork. Pinning it to
        `startup` would silently drop the persona digest after the first /compact."""
        h = _hooks_json()
        for entry in h["SessionStart"]:
            self.assertNotIn("matcher", entry)

    def test_plugin_declares_every_hook_the_control_plane_used_to_wire_by_hand(self):
        """Parity check against the pre-plugin wiring, enumerated explicitly rather than
        read from a sibling control-plane checkout's .claude/settings.json: charter
        is a public repo (github.com/diazoxide/charter) whose CI and other contributors
        never have that sibling checkout, so a test that stat()s another repo's absolute
        path would be unrunnable (or silently vacuous) everywhere but this machine.

        Asserted as (event, command) pairs rather than a flat count, so the same handler
        can legitimately appear on two events — `_autosave` runs on both `Stop` and
        `SubagentStop`, because a dispatched sub-agent finishing fires only the latter
        and its workspace memo would otherwise wait for the parent session to end.
        Pairing also catches a handler that drifts onto the wrong event, which a count
        never could."""
        pairs = {(ev, hook["command"]) for ev, hook in _flat_hooks()}
        expected = [
            ("SessionStart", "charter workspace _reconcile"),   # reconcile workspace lock/pointer
            ("SessionStart", "charter persona _gc"),             # prune ended-session scratch memory
            ("SessionStart", "charter hook sessionstart --plugin-version"),  # memory digest injection
            ("SessionStart", "charter doctor"),                  # preflight, message only on failure
            ("SessionStart", "charter gl-refresh"),              # refresh forge state
            ("UserPromptSubmit", "charter hook userpromptsubmit --plugin-version"),
            ("PreToolUse", "charter hook pretooluse --plugin-version"),
            # The vault guard for file-reading TOOLS. A separate matcher rather than folding
            # it into `pretooluse`, because that one is registered for Bash and reads
            # `tool_input["command"]` — a Read carries no command and would reach none of it
            # (#90). Registered for Read|Grep only: Glob returns names, not contents.
            ("PreToolUse", "charter hook pretooluse-read --plugin-version"),
            ("PreToolUse", "charter hook pretooluse-dispatch --plugin-version"),
            # `routing: require`'s tool-time half. Registered for the WRITE tools only:
            # the ask is about editing without having dispatched, and a Read that triggered
            # it would fire on the scouting the gate asks for two lines earlier.
            ("PreToolUse", "charter hook pretooluse-edit --plugin-version"),
            ("PostToolUse", "charter hook posttooluse --plugin-version"),
            # The other half of every `ask`: a PostToolUse for the asked-about tool_use_id
            # is proof the tool ran, i.e. that a human approved it. Registered for Bash
            # because that is where charter's asks are emitted — 231 of them went out with
            # no outcome ever recorded, which is what made the guard unarguable (#290).
            ("PostToolUse", "charter hook posttooluse-bash --plugin-version"),
            # The skill tally: which skills a persona actually invokes, against the ones its
            # charter declares and the host preloads on every dispatch.
            ("PostToolUse", "charter hook posttooluse-skill --plugin-version"),
            ("PostToolUse", "charter hook posttooluse-dispatch --plugin-version"),
            # Resumes: more work handed to a persona already running. Its own matcher
            # because `Task|Agent` fires when a sub-agent is CREATED and never again.
            ("PostToolUse", "charter hook posttooluse-message --plugin-version"),
            # The turn's FALLING edge (#853). `Stop` and NOT `SubagentStop`: a sub-agent
            # finishing does not end the turn that dispatched it, so clearing the chat's
            # working mark there would blink the tab off mid-fan-out. It rides the same
            # entry as the autosave and goes FIRST — the autosave carries a 15s timeout
            # and this is a readout the operator is looking at.
            ("Stop", "charter hook stop --plugin-version"),
            ("Stop", "charter workspace _autosave"),             # debounced auto-save
            ("SubagentStop", "charter workspace _autosave"),     # ditto, per dispatch
        ]
        self.assertEqual(len(pairs), len(expected), sorted(pairs))
        for event, substr in expected:
            matches = [c for ev, c in pairs if ev == event and substr in c]
            self.assertEqual(len(matches), 1, f"{event}: {substr} -> {matches}")

    def test_the_session_never_waits_on_the_background_calls(self):
        """persona _gc and gl-refresh are network-touching background calls at session
        start; a session must not wait on them synchronously.

        This used to assert `"async": true`, which pinned the MECHANISM rather than the
        intent. Codex loads charter's plugin and skips async entries outright — printing
        `async hooks are not supported yet` twice a session — so the work simply never
        happened there. The commands now detach themselves, which is charter's own code
        and needs nothing from the host, and the manifest asks for no async at all.
        """
        cmds = [hook["command"] for _, hook in _flat_hooks()]
        for needle in ("persona _gc", "gl-refresh"):
            matching = [c for c in cmds if needle in c]
            self.assertEqual(len(matching), 1, f"{needle} -> {matching}")
            self.assertIn("--detach", matching[0])
        self.assertEqual([h for _, h in _flat_hooks() if "async" in h], [],
                         "a harness that skips async entries loses them silently")
        # A detached command returns at once, so a timeout on it would be meaningless.
        for _, hook in _flat_hooks():
            if "--detach" in hook.get("command", ""):
                self.assertNotIn("timeout", hook)

    def test_timed_hooks_keep_their_original_timeout(self):
        """Losing a timeout can hang session start on a stuck subprocess.

        Engine commands are keyed by their handler name and the version is interpolated,
        not typed. Spelling the whole command out froze `--plugin-version 0.1.0` into a
        test about *timeouts*, so a legitimate version bump failed here for a reason that
        has nothing to do with what this asserts — and the fix looked like editing a
        version string in a test, which is how a stale one gets waved through.
        """
        engine = {"sessionstart": 5, "userpromptsubmit": 5, "pretooluse": 10,
                  "posttooluse": 5, "posttooluse-dispatch": 5, "stop": 5}
        expected = {f"charter hook {name} --plugin-version {__version__}": t
                    for name, t in engine.items()}
        expected["charter workspace _reconcile >/dev/null 2>&1"] = 5
        expected["charter workspace _autosave >/dev/null 2>&1"] = 15

        by_cmd = {hook["command"]: hook for _, hook in _flat_hooks()}
        for cmd, timeout in expected.items():
            self.assertIn(cmd, by_cmd)
            self.assertEqual(by_cmd[cmd].get("timeout"), timeout, cmd)

    def test_doctor_preflight_keeps_its_fallback_and_status_message(self):
        """Quiet on success, prints the captured output only on failure (the `||`
        fallback) — and carries the statusMessage the umbrella showed during the check."""
        doctor = [hook for event, hook in _flat_hooks()
                 if event == "SessionStart" and "charter doctor" in hook["command"]]
        self.assertEqual(len(doctor), 1)
        cmd = doctor[0]["command"]
        self.assertIn("||", cmd)
        self.assertIn("charter doctor", cmd)
        self.assertEqual(doctor[0].get("timeout"), 20)
        self.assertTrue(doctor[0].get("statusMessage"))


class TestMarketplaceManifest(unittest.TestCase):
    """F1 — `claude plugin marketplace add <owner>/<repo>` requires
    `.claude-plugin/marketplace.json`; without it `claude plugin install charter@charter`
    cannot work at all, no matter how correct `plugin.json` is. charter is its own
    single-plugin marketplace (`"source": "./"`, mirroring the shape of e.g.
    `mattpocock/skills`'s own marketplace.json), so these two manifests must never drift
    apart: the marketplace's plugin entry has to name the SAME plugin `plugin.json`
    declares, and has to source it from the repo root."""

    def _manifest(self) -> dict:
        return json.loads((ROOT / ".claude-plugin" / "plugin.json").read_text())

    def _marketplace(self) -> dict:
        return json.loads((ROOT / ".claude-plugin" / "marketplace.json").read_text())

    def test_marketplace_manifest_exists_and_is_valid_json(self):
        self._marketplace()  # must not raise

    def test_marketplace_declares_exactly_one_plugin(self):
        m = self._marketplace()
        self.assertEqual(len(m.get("plugins") or []), 1, m.get("plugins"))

    def test_marketplace_plugins_name_matches_plugin_json_name(self):
        """The anti-drift assertion: if `plugin.json`'s `name` ever changes without a
        matching edit here, `charter@charter` stops resolving — catch it in CI, not in
        a user's failed `claude plugin install`."""
        plugin = self._manifest()
        entry = self._marketplace()["plugins"][0]
        self.assertEqual(entry["name"], plugin["name"])

    def test_marketplace_plugin_source_points_at_the_repo_root(self):
        entry = self._marketplace()["plugins"][0]
        self.assertEqual(entry["source"], "./")

    def test_marketplace_and_plugin_entries_carry_a_description(self):
        m = self._marketplace()
        self.assertTrue(m.get("description"))
        self.assertTrue(m["plugins"][0].get("description"))


class TestVersionsMoveInLockstep(unittest.TestCase):
    """The two artifacts carry two version numbers, and `hooks.MIN_PLUGIN_VERSION` is
    simply `charter.__version__` — so the plugin's numbers are only meaningful while
    somebody keeps them equal.

    Nobody did. The CLI reached 0.13.1 while `plugin.json` and all six `--plugin-version`
    flags still said 0.1.0, and the comment above `MIN_PLUGIN_VERSION` went on claiming
    the two were "bumped in lockstep". Nothing caught it because the only test that looked
    at those flags checked that they were PRESENT, never what they said — and the skew
    guard is deliberately one-directional (it fires when the plugin is NEWER than the CLI),
    so a plugin frozen years behind stays silent forever.

    A release now touches four files: pyproject.toml, charter/__init__.py,
    .claude-plugin/plugin.json, and every `--plugin-version` in hooks/hooks.json. These
    tests name all four, so forgetting one fails here instead of shipping.
    """

    def test_plugin_manifest_matches_the_cli(self):
        m = json.loads((ROOT / ".claude-plugin" / "plugin.json").read_text())
        self.assertEqual(
            m["version"], __version__,
            f".claude-plugin/plugin.json says {m['version']}, the CLI is {__version__} — "
            f"a release bumps pyproject.toml, charter/__init__.py, plugin.json and "
            f"hooks/hooks.json together.")

    def test_every_hook_command_passes_the_cli_version(self):
        """The value the running hook actually hands to `skew_message`. A stale one here
        is worse than a stale manifest: this is what the guard compares against."""
        engine_cmds = [hook["command"] for _, hook in _flat_hooks()
                       if "charter hook " in hook["command"]]
        self.assertTrue(engine_cmds, "manifest declares no engine hooks — did they move?")
        for c in engine_cmds:
            self.assertIn(f"--plugin-version {__version__}", c,
                          f"stale --plugin-version in hooks/hooks.json: {c!r}")

    def test_the_shipped_plugin_version_is_silent_against_its_own_cli(self):
        """The end-to-end statement: install both from this commit and no hook shouts."""
        m = json.loads((ROOT / ".claude-plugin" / "plugin.json").read_text())
        self.assertIsNone(hooks.skew_message(m["version"]))


class TestVersionSkew(unittest.TestCase):
    def test_a_matching_version_is_silent(self):
        self.assertIsNone(hooks.skew_message(__version__))

    def test_a_newer_plugin_than_cli_says_exactly_what_to_run(self):
        msg = hooks.skew_message("99.0.0")
        self.assertIsNotNone(msg)
        self.assertIn("charter", msg)
        self.assertIn("upgrade", msg.lower())

    def test_an_absent_plugin_version_is_silent(self):
        """Someone invoking the hook by hand must not be nagged."""
        self.assertIsNone(hooks.skew_message(None))

    def test_a_malformed_version_does_not_crash(self):
        self.assertIsNone(hooks.skew_message("not-a-version"))


class TestSkewReachesTheUser(unittest.TestCase):
    # `hooks.dispatch` runs REAL handlers, and a handler may write plane state — a trace
    # row, a guard-seen mark. Without redirecting the root those writes land in whatever
    # plane this checkout resolves to, which since `find_root` began hopping outward
    # through `workspaces/` (0.37.0) is the developer's own control plane. A test must not
    # be able to touch it; `config.use` is the seam the rest of the suite already uses.
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        import tempfile
        from charter import config as _config
        self._tmp = tempfile.mkdtemp(prefix="charter-skew-")
        (Path(self._tmp) / "charter.toml").write_text("schema = 1\n")
        self._orig = _config.use(Path(self._tmp))

    def tearDown(self) -> None:
        import shutil
        from charter import config as _config
        _config.restore(self._orig)
        shutil.rmtree(self._tmp, ignore_errors=True)

    """`README.md` promises "a plugin newer than the CLI says so loudly at session start".
    It did not. `dispatch` printed to stderr and returned 0, and Claude Code routes a
    zero-exit hook's stderr to the debug log only — so neither the user nor the model ever
    saw it. The second surface was no better: `check_plugin_skew` returned WARN, and
    `cmd_doctor` exits 0 on WARN, so the `||` in hooks.json never fired either."""

    def _dispatch(self, name, version):
        import io as _io, json as _json
        from contextlib import redirect_stdout
        from tests._isolation import run_hook  # noqa: F401  (stdin plumbing)
        buf = _io.StringIO()
        old = sys.stdin
        sys.stdin = _io.StringIO("{}")
        try:
            with redirect_stdout(buf):
                hooks.dispatch(name, version)
        finally:
            sys.stdin = old
        out = buf.getvalue().strip()
        return _json.loads(out) if out else None

    def test_session_start_surfaces_it_as_a_system_message(self):
        got = self._dispatch("sessionstart", "99.0.0")
        self.assertIsNotNone(got, "dispatch emitted nothing")
        self.assertIn("systemMessage", got)
        self.assertIn("version skew", got["systemMessage"])

    def test_a_matching_version_says_nothing(self):
        got = self._dispatch("sessionstart", __version__)
        self.assertFalse((got or {}).get("systemMessage"))

    def test_pretooluse_does_not_repeat_it_on_every_bash_call(self):
        """Emitting there would print the same warning dozens of times a session, which is
        how a guard stops working even once it is finally visible."""
        got = self._dispatch("pretooluse", "99.0.0")
        self.assertFalse((got or {}).get("systemMessage"))

    def test_doctor_treats_skew_as_a_blocker(self):
        """`cmd_doctor` exits 1 only on FAIL, and that exit code is what makes the
        SessionStart wrapper print anything at all."""
        import os
        from charter import doctor
        from unittest import mock
        with mock.patch.dict(os.environ, {"CLAUDE_PLUGIN_ROOT": str(ROOT)}):
            res = doctor.check_plugin_skew()
        self.assertEqual(res.status, doctor.OK)   # this repo is in lockstep

    def test_a_newer_plugin_is_a_fail_not_a_warn(self):
        from charter import doctor
        from unittest import mock
        with mock.patch.object(hooks, "MIN_PLUGIN_VERSION", "0.0.1"), \
             mock.patch.dict(__import__("os").environ, {"CLAUDE_PLUGIN_ROOT": str(ROOT)}):
            res = doctor.check_plugin_skew()
        self.assertEqual(res.status, doctor.FAIL)


if __name__ == "__main__":
    unittest.main()
