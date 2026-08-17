"""The generated plugin is the one artifact charter ships and never runs.

A syntax error in it would reach every user with the suite green — the failure shape this
repo has paid for twice (#177, #197). So the suite runs it: Bun if present (opencode's own
runtime), Node otherwise, and skips only when neither is installed rather than passing
quietly.
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from charter.harness import opencode

_RUNTIME = shutil.which("bun") or shutil.which("node")


@unittest.skipIf(_RUNTIME is None, "neither bun nor node is installed")
class ShimParses(unittest.TestCase):
    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="charter-shim-js-"))
        self.addCleanup(lambda: shutil.rmtree(self.dir, True))

    def _write(self, suffix: str) -> Path:
        p = self.dir / f"charter{suffix}"
        p.write_text(opencode.SHIM)
        return p

    def test_it_is_syntactically_valid(self):
        """Node cannot parse TypeScript, but the shim deliberately carries no types — it
        imports nothing and annotates nothing, so it is valid JavaScript as written. That
        is a property worth pinning: the day someone adds a type annotation, this fails
        rather than the user's session."""
        src = self._write(".mjs")
        cmd = ([_RUNTIME, "build", "--target=node", str(src), "--outfile=/dev/null"]
               if _RUNTIME.endswith("bun") else [_RUNTIME, "--check", str(src)])
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        self.assertEqual(proc.returncode, 0,
                         f"the generated plugin does not parse:\n{proc.stderr}")

    def test_the_factory_returns_the_hooks_opencode_will_call(self):
        """Imported and called for real, so the export shape is checked rather than
        pattern-matched. `$` and `directory` are what opencode passes."""
        src = self._write(".mjs")
        probe = self.dir / "probe.mjs"
        probe.write_text(
            f"import {{ CharterPlugin }} from './{src.name}';\n"
            "const hooks = await CharterPlugin({ $: null, directory: '/tmp' });\n"
            "console.log(Object.keys(hooks).sort().join(','));\n")
        proc = subprocess.run([_RUNTIME, str(probe)], capture_output=True, text=True,
                              timeout=120, cwd=self.dir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.strip(),
                         "shell.env,tool.execute.after,tool.execute.before")


    def _drive(self, reply: str, tool: str = "bash") -> subprocess.CompletedProcess:
        """Call `tool.execute.before` with a `$` that implements ONLY what Bun's shell
        implements: a tagged template returning a thenable with `.env`, `.quiet` and
        `.nothrow` — and **no `.stdin`**.

        This is the test the first version needed and did not have. The shim called
        `.stdin(...)`, which does not exist, so it threw on every tool call and failed
        open: the guard never fired while `doctor` reported everything wired. A stub that
        mirrors the real surface turns that into a red test instead of a silent hole.
        """
        src = self._write(".mjs")
        probe = self.dir / "drive.mjs"
        probe.write_text(f"""
import {{ CharterPlugin }} from './{src.name}';
const $ = (strings, ...vals) => {{
  const p = Promise.resolve({{ stdout: Buffer.from({reply!r}) }});
  p.env = () => p; p.quiet = () => p; p.nothrow = () => p;
  return p;
}};
const hooks = await CharterPlugin({{ $, directory: '/tmp' }});
try {{
  await hooks['tool.execute.before']({{ tool: {tool!r}, sessionID: 's' }},
                                     {{ args: {{ command: 'x' }} }});
  console.log('ALLOWED');
}} catch (e) {{ console.log('DENIED: ' + e.message); }}
""")
        return subprocess.run([_RUNTIME, str(probe)], capture_output=True, text=True,
                              timeout=120, cwd=self.dir)

    def test_a_deny_from_charter_throws_and_carries_the_reason(self):
        reply = ('{"hookSpecificOutput":{"permissionDecision":"deny",'
                 '"permissionDecisionReason":"charter guard: token-only"}}')
        proc = self._drive(reply)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.strip(), "DENIED: charter guard: token-only")

    def test_no_decision_lets_the_tool_run(self):
        proc = self._drive('{"hookSpecificOutput":{"hookEventName":"PreToolUse"}}')
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.strip(), "ALLOWED")

    def test_silence_from_charter_lets_the_tool_run(self):
        """An unwired plane, or a charter that is not installed: the guard must fail OPEN.
        A guard that cannot run must not make the harness unusable."""
        proc = self._drive("")
        self.assertEqual(proc.stdout.strip(), "ALLOWED")


if __name__ == "__main__":
    unittest.main()
