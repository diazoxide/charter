"""``.charter/`` is 0700 because charter says so, not because of the operator's umask.

`base.make_private_dir` created every level of a **vault** path at 0700, and the state
directory is not one of those levels on any flow that reaches it first. `charter vault
add` writes the local registry through a bare ``mkdir(parents=True, exist_ok=True)`; the
SessionStart hook writes a workspace pointer; the PreToolUse hook writes ``guard-seen.json``;
the status line writes a cache. Whichever ran first decided the mode, at ``0o777 & ~umask``
— 0755 on the default ``umask 022``. So every account on the machine could list the plane's
state directory, and *which* command you happened to run first decided whether it could
(#470).

**The property is "the umask does not decide it", not "the mode is 0700".** Those come
apart: a fix that hardcoded 0755 would satisfy "the mode is a constant" and be no fix at
all, and one that only works under `umask 022` satisfies nothing. Every case below runs the
same flow under three umasks — ``000`` (0777 by default), ``022`` (the default) and ``077``
(already private) — and asserts the same private mode came out of all three. Modes are
tested through ``mode & 0o077``, never against a list of known-bad values: 0755 is the one
everybody pictures, while 0705, 0711, 0730 and 0701 list or traverse just as well.

**Two halves, because either alone is a fix that looks whole.**

*Behaviour* is the CLI, in a subprocess, in a plane that has no ``.charter/`` yet — a fresh
clone of a control plane, which is the ordinary case rather than an exotic one, since
``.charter/`` is gitignored. Three different writers get the first move, because the defect
was never in one of them: it was in every writer that reached the state directory without
going through the walk.

*Coverage* is `tests/_statedirscan.py`, which reads the package and asks whether any
``mkdir`` left in it can still create a directory under the state directory without going
through `config.private_mkdir`. A behavioural sweep can only ever cover the writers
somebody thought to run; this is what notices the fourth one. Its own accuracy is tested
here against sources built for the purpose, so a scanner that has quietly stopped seeing
anything cannot report a clean package.

**The next spelling.** A path that reaches a writer as a parameter, or one assembled from
a string, is invisible to the scanner — it says so itself. What keeps that from being an
exposure is that the level those paths hang off, ``.charter/`` itself, is 0700: a
directory created under it at the umask default is still reachable by nobody but its
owner. The scan is about keeping the walk honest, not about being the guard.

`charter statusline` is deliberately NOT one of the commands swept: it forks a detached
``charter _version-check``, and no test in this suite makes a network call, directly or by
proxy. It reaches the state directory through `update.maybe_spawn`'s lock file, which the
scan covers.
"""

from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from charter import config

from tests import _statedirscan as scan
from tests._isolation import PersonaIso

REPO_ROOT = Path(__file__).resolve().parent.parent

#: ``000`` makes a bare mkdir 0777, ``022`` is the default that shipped the defect, ``077``
#: is the one under which the old code was accidentally right. A fix has to produce the
#: same private mode under all three — the property is the independence, not the value.
UMASKS = (0o000, 0o022, 0o077)


def modes_up_to(leaf, stop) -> dict:
    """``{path: mode}`` for every directory from *leaf* up to and including *stop*."""
    out, cur, stop_rp = {}, Path(leaf), Path(stop).resolve()
    while True:
        out[cur] = stat.S_IMODE(cur.stat().st_mode)
        if cur.resolve() == stop_rp or cur.parent == cur:
            return out
        cur = cur.parent


class TheCliDecidesIt(unittest.TestCase):
    """The real binary, in a real plane, with no ``.charter/`` in it yet.

    A subprocess rather than a handler call, because the umask is a property of the
    process and because the defect was in the *order commands run in* — which is a thing
    only the CLI actually has.
    """

    #: One plane, built once by `charter init` and copied per case. Building it per case
    #: costs a second each; copying is free, and `init` creates no `.charter/`, so the
    #: template cannot smuggle a mode into a case.
    template: Path

    @classmethod
    def setUpClass(cls) -> None:
        cls._root = Path(tempfile.mkdtemp(prefix="charter-statedir-"))
        cls.template = cls._root / "template"
        cls.template.mkdir()
        env = cls.child_env()
        r = subprocess.run(
            [sys.executable, "-m", "charter", "init", "--forge", "github",
             "--owner", "acme", "--no-front-door"],
            cwd=cls.template, env=env, text=True, capture_output=True, timeout=120,
            stdin=subprocess.DEVNULL)
        if r.returncode != 0:
            raise AssertionError(f"could not build the fixture plane:\n{r.stdout}\n{r.stderr}")
        if (cls.template / ".charter").exists():
            raise AssertionError(
                "`charter init` now creates `.charter/` itself. That is not wrong, but it "
                "makes every case below start from a state directory the template chose — "
                "copy the plane WITHOUT it, or these tests stop measuring anything.")

    @classmethod
    def tearDownClass(cls) -> None:
        shutil.rmtree(cls._root, ignore_errors=True)

    @staticmethod
    def child_env() -> dict:
        """The child's environment: this checkout on the path, and every charter variable
        that could redirect the state directory removed, so the plane under test is the
        temp one and nothing reaches the developer's own."""
        env = dict(os.environ)
        env["PYTHONPATH"] = str(REPO_ROOT)
        for var in ("CHARTER_HOME", "CHARTER_PERSONA", "CHARTER_WORKSPACE",
                    "CHARTER_WORKTREES", "CHARTER_CONFIG_HOME"):
            env.pop(var, None)
        return env

    def plane(self, tag: str) -> Path:
        d = self._root / tag
        shutil.copytree(self.template, d)
        self.assertFalse((d / ".charter").exists(), "precondition: charter creates it")
        return d

    def charter(self, plane: Path, *args: str, stdin: str = "") -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, "-m", "charter", *args], cwd=plane, env=self.child_env(),
            input=stdin, text=True, capture_output=True, timeout=120)

    def state_mode(self, plane: Path) -> int:
        sd = plane / ".charter"
        self.assertTrue(
            sd.is_dir(),
            "this command no longer creates `.charter/`, so the case proves nothing about "
            "the mode of a directory nobody made. Pick a command that does.")
        return stat.S_IMODE(sd.stat().st_mode)

    def _sweep(self, label: str, args: tuple, stdin: str = "") -> None:
        seen = {}
        for um in UMASKS:
            with self.subTest(umask=oct(um)):
                plane = self.plane(f"{label}-{um:03o}")
                old = os.umask(um)
                try:
                    proc = self.charter(plane, *args, stdin=stdin)
                finally:
                    os.umask(old)
                self.assertNotIn("Traceback (most recent call last):",
                                 (proc.stdout or "") + (proc.stderr or ""),
                                 f"the command crashed:\n{proc.stdout}\n{proc.stderr}")
                mode = self.state_mode(plane)
                seen[um] = mode
                self.assertEqual(
                    mode & 0o077, 0,
                    f"under umask {oct(um)}, `charter {' '.join(args)}` left `.charter` at "
                    f"{oct(mode)[-3:]} — another account on this machine can reach the "
                    f"plane's state directory")
        self.assertEqual(
            len(set(seen.values())), 1,
            f"the umask still decides it: {[(oct(u), oct(m)[-3:]) for u, m in seen.items()]}")

    def test_vault_add_is_the_flow_from_the_issue(self) -> None:
        """`charter vault add` writes the local registry first, and that write is what
        created `.charter/` at the umask default on the flow the issue reproduces."""
        self._sweep("vaultadd", ("vault", "add", "devops", "--provider", "plain-file"))

    def test_the_session_start_hook_gets_there_first_on_a_fresh_clone(self) -> None:
        """The ordinary case: `.charter/` is gitignored, so a teammate who clones the
        plane has none, and the first thing that runs is a hook — which writes a workspace
        pointer under `.charter/sessions/`, not through any vault writer."""
        payload = json.dumps({"session_id": "sess-470", "cwd": "."})
        self._sweep("sessionstart", ("hook", "sessionstart"), stdin=payload)

    def test_the_pretooluse_hook_gets_there_first(self) -> None:
        """A third writer, and a third file: `guard-seen.json` sits directly in
        `.charter/`. Three independent paths, because the defect was never in one writer —
        it was in every writer that reached the state directory without the walk."""
        payload = json.dumps({"session_id": "sess-470", "cwd": ".",
                              "tool_name": "Bash", "tool_input": {"command": "ls"}})
        self._sweep("pretooluse", ("hook", "pretooluse"), stdin=payload)

    def test_an_existing_loose_state_directory_is_left_exactly_as_it_is(self) -> None:
        """The other half, and it is not a compromise: charter tightens what it creates and
        reports what it did not. `$CHARTER_HOME` can point the state directory at any path
        on the machine, so "chmod whatever we land in" is how charter would come to tighten
        a home or a shared team directory unprompted (#331)."""
        plane = self.plane("preexisting")
        sd = plane / ".charter"
        sd.mkdir()
        os.chmod(sd, 0o755)
        self.charter(plane, "vault", "add", "devops", "--provider", "plain-file")
        self.assertEqual(stat.S_IMODE(sd.stat().st_mode), 0o755,
                         "charter chmod-ed a directory it did not create")


class ThePrivateWalkItself(PersonaIso):
    """`config.private_mkdir`, at the level the CLI sweep cannot reach."""

    def test_every_level_it_creates_is_private(self) -> None:
        """The defect the first cut of #437 shipped: ``mkdir(parents=True, mode=0o700)``
        applies *mode* to the leaf only and creates the parents at the umask default."""
        for um in UMASKS:
            with self.subTest(umask=oct(um)):
                old = os.umask(um)
                self.addCleanup(os.umask, old)
                root = Path(self.tmp) / f"walk-{um:03o}"
                leaf = root / "a" / "b" / "c"
                config.private_mkdir(leaf)
                chain = modes_up_to(leaf, root)
                self.assertGreaterEqual(len(chain), 3, "a short chain makes this vacuous")
                for p, mode in chain.items():
                    self.assertEqual(mode & 0o077, 0, f"{p} came out {oct(mode)[-3:]}")

    def test_an_existing_directory_keeps_its_mode(self) -> None:
        d = Path(self.tmp) / "pre"
        d.mkdir()
        os.chmod(d, 0o755)
        config.private_mkdir(d)
        self.assertEqual(stat.S_IMODE(d.stat().st_mode), 0o755)

    def test_the_leaf_is_attempted_before_the_parents(self) -> None:
        """A leaf that cannot exist must not leave its parents standing behind it.

        `frame.state` counts a respawn against a directory `reap` may have deleted, and
        pins "does not raise **or create**". A walk that made the parents on the way down
        would resurrect a frame root under whoever had just reaped it — so the order is
        `pathlib`'s: leaf first, parents only on ``FileNotFoundError``.
        """
        root = Path(self.tmp) / "leaffirst"
        overlong = root / "missing" / ("x" * 5000)
        with self.assertRaises(OSError):
            config.private_mkdir(overlong)
        self.assertFalse(root.exists(),
                         "the parents were created for a leaf that could never exist")

    def test_parents_false_refuses_to_build_the_path(self) -> None:
        root = Path(self.tmp) / "noparents"
        with self.assertRaises(FileNotFoundError):
            config.private_mkdir(root / "a" / "b", parents=False)
        self.assertFalse(root.exists())

    def test_a_file_in_the_way_is_still_an_error(self) -> None:
        """``mkdir(exist_ok=True)`` raises when the path exists and is not a directory, and
        every caller here writes into the path afterwards. Swallowing it would turn a
        `FileExistsError` into a confusing failure one line later."""
        f = Path(self.tmp) / "not-a-dir"
        f.write_text("x")
        with self.assertRaises(FileExistsError):
            config.private_mkdir(f)

    def test_the_vault_writers_call_the_same_walk(self) -> None:
        """One implementation, two names — so a fix to one cannot miss the other."""
        from charter.secrets import base

        calls = []
        original = config.private_mkdir
        config.private_mkdir = lambda p, *a, **kw: calls.append(Path(p))
        try:
            base.make_private_dir(Path(self.tmp) / "via-secrets")
        finally:
            config.private_mkdir = original
        self.assertEqual(calls, [Path(self.tmp) / "via-secrets"])


class TheScanSeesWhatItClaims(unittest.TestCase):
    """The coverage scanner's own accuracy, against sources written for the purpose.

    A scan that has quietly stopped seeing anything reports a clean package, which is the
    most comfortable way for this whole file to become decorative.
    """

    def setUp(self) -> None:
        self.names = scan.state_attribute_names()

    def test_the_state_names_are_asked_of_config(self) -> None:
        """Derived from `config.derive`, not listed here: a setting added under the state
        directory is covered the day it is added."""
        self.assertIn("STATE_DIR", self.names)
        self.assertIn("SESSIONS_DIR", self.names)
        self.assertNotIn("PERSONAS_DIR", self.names, "personas/ is committed, not state")
        self.assertNotIn("ROOT", self.names)

    def test_a_bare_mkdir_on_the_state_directory_is_caught(self) -> None:
        src = "def f():\n    (config.STATE_DIR / 'x').mkdir(parents=True, exist_ok=True)\n"
        self.assertEqual([ln for ln, _ in scan.violations(src, self.names)], [2])

    def test_the_module_alias_does_not_hide_it(self) -> None:
        """`hooks` reaches config as `_cfg`. A scan keyed to the alias would skip it."""
        src = "def f():\n    (_cfg.STATE_DIR / 'x').mkdir(parents=True)\n"
        self.assertEqual([ln for ln, _ in scan.violations(src, self.names)], [2])

    def test_one_hop_of_indirection_does_not_hide_it(self) -> None:
        """The shape most of the writers actually have: a module-level path helper, then
        ``f.parent.mkdir(...)`` in the function that writes."""
        src = ("def _cache_file():\n    return config.STATE_DIR / 'cache' / 'x.json'\n\n"
               "def save():\n    f = _cache_file()\n"
               "    f.parent.mkdir(parents=True, exist_ok=True)\n")
        self.assertEqual([ln for ln, _ in scan.violations(src, self.names)], [6])

    def test_a_directory_outside_the_state_tree_is_not_flagged(self) -> None:
        """A scan that flagged everything would be as useless as one that flagged
        nothing — and would be "fixed" by making committed directories private."""
        src = ("def _p():\n    return config.PERSONAS_DIR / 'x'\n\n"
               "def f():\n    _p().mkdir(parents=True, exist_ok=True)\n")
        self.assertEqual(scan.violations(src, self.names), [])

    def test_the_routed_call_is_not_flagged(self) -> None:
        src = ("def f():\n    config.private_mkdir(config.STATE_DIR / 'cache')\n")
        self.assertEqual(scan.violations(src, self.names), [])


class EveryStateWriterGoesThroughTheWalk(unittest.TestCase):
    def test_no_mkdir_in_the_package_can_make_a_loose_state_directory(self) -> None:
        found = scan.scan_package()
        self.assertEqual(
            found, {},
            "these create a directory under `.charter/` without `config.private_mkdir`, "
            "so whichever of them runs first in a fresh plane hands the umask the mode of "
            "the state directory (#470):\n"
            + "\n".join(f"  {f}:{ln}  {expr}.mkdir(…)"
                        for f, hits in found.items() for ln, expr in hits))


if __name__ == "__main__":
    unittest.main()
