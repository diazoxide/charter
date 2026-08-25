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
``.charter/`` is gitignored. Four different writers get the first move, because the defect
was never in one of them: it was in every writer that reached the state directory without
going through the walk — and the fourth, ``persona remember --ephemeral``, is the one no
reader of the package could have named (see below).

*Coverage* is `tests/_statedirscan.py`, which reads the package and asks whether any
``mkdir`` left in it can still create a directory under the state directory without going
through `config.private_mkdir` or `config.mkdir_for`. A behavioural sweep can only ever
cover the writers somebody thought to run; this is what notices the fourth one. Its own
accuracy is tested here against sources built for the purpose, so a scanner that has
quietly stopped seeing anything cannot report a clean package.

**The spelling this file used to excuse, and the property that replaced it.** The first
cut of the paragraph above said a path reaching a writer as a *parameter* was invisible to
the scan, and that this was safe "because the level those paths hang off, ``.charter/``
itself, is 0700". That reasoning was wrong on the exact flow that exercises it. On a fresh
clone ``.charter/`` is gitignored and absent, so ``charter persona remember <p> "…"
--ephemeral`` — whose directory reaches `memstore.write` as a parameter — is what *creates
the state directory itself*, at 0755 under ``umask 022`` and 0777 under ``umask 000``.
There was no 0700 level above it to hang off. Two things changed together, and either
alone would have been half a fix:

- `config.mkdir_for` decides at **runtime**, on where the path actually is, so a writer
  that is handed its directory is right about a path no reader could have named. It is a
  dispatch, not a privatiser: a committed directory passed to it keeps the operator's
  umask, which `TheDispatchOnWhereThePathIs` and the committed CLI case both pin.
- the scan stopped matching the spelling ``.STATE_DIR`` at the call site and started
  asking **reachability** — a call that passes a state path into a package function taints
  that parameter, transitively, and a ``mkdir`` on a tainted parameter is a violation
  exactly as a ``mkdir`` on ``config.STATE_DIR / …`` is. Put `memstore`'s bare mkdir back
  and the scan names it.

**The next spelling after this one.** A path assembled from a string
(``Path(str(config.ROOT) + "/.charter")``), or arriving from outside the package — read
out of JSON, taken from ``argv`` — names no attribute and makes no call, so the reader
cannot see it and this time the scan says so without an excuse attached. What answers it
is `config.mkdir_for`, at runtime. The scan's job is to prove every writer is routed; the
guard is `config`.

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
        # A persona, written as plain files rather than by `charter persona create`, so
        # the template still cannot smuggle a `.charter/` into every case. The ephemeral
        # sweep below needs one to exist; nothing else here reads it.
        d = cls.template / "personas" / "steward"
        d.mkdir(parents=True)
        (d / "persona.md").write_text(
            "---\nname: steward\nrole: Steward\n---\n\n# Steward\n\ncharter body\n")
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
        temp one and nothing reaches the developer's own.

        ``CHARTER_ROOT`` belongs on that list and was missing from it. Isolation here is by
        ``cwd`` — each case runs the CLI standing inside its own copied plane — and
        ``$CHARTER_ROOT`` wins outright over that walk (`root.find_root`). Every charter
        frame exports it, so on the machine most likely to run these, `setUpClass` ran
        ``charter init`` against the operator's own control plane. It never showed up
        because nobody ran the suite from inside a frame with the variable set; the suite's
        spawn tripwire refuses it now (#527), which is how it was found.
        """
        env = dict(os.environ)
        env["PYTHONPATH"] = str(REPO_ROOT)
        for var in ("CHARTER_ROOT", "CHARTER_HOME", "CHARTER_PERSONA", "CHARTER_WORKSPACE",
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

    def _sweep(self, label: str, args: tuple, stdin: str = "",
               wrote: str | None = None) -> None:
        """Run *args* under each umask in a fresh plane and assert one private mode.

        *wrote* is a glob the command must have left behind. Without it a case passes as
        long as `.charter/` came out private — including when the command it names failed
        outright and some *other* writer in the same process (a trace line, a session
        marker) made the directory. That is the shape where a fixture goes wrong silently
        and the sweep keeps saying OK.
        """
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
                if wrote is not None:
                    self.assertTrue(
                        list(plane.glob(wrote)),
                        f"`charter {' '.join(args)}` left nothing at {wrote}, so whatever "
                        f"made `.charter/` here was not the writer this case names:\n"
                        f"{proc.stdout}\n{proc.stderr}")
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

    def test_ephemeral_memory_gets_there_first(self) -> None:
        """The fourth writer — and the one the scan could not see.

        `persona remember … --ephemeral` writes under ``PERSONA_STATE_DIR``, and the path
        reaches the writer as a **parameter**: `memstore.write(mem_dir, …)` is handed the
        committed ``personas/<n>/memory`` on one call and this gitignored one on the next.
        No line in `memstore` spells a state name, so a scan matching the spelling saw a
        clean package while this command created `.charter/` itself at the umask default
        (#470). `config.mkdir_for` asks the question the caller alone could answer.
        """
        self._sweep("ephem", ("persona", "remember", "steward",
                              "an ordinary scratch note", "--ephemeral"),
                    wrote=".charter/persona-state/ephemeral/*/steward/*.md")

    def test_the_committed_quadrant_is_not_tightened(self) -> None:
        """The other half of the same dispatch, and not a footnote: a `mkdir_for` that
        made everything private would pass the sweep above and quietly turn committed
        persona directories 0700 — the same overreach as chmod-ing a `.charter/` charter
        did not create (#331). Measured against a control directory made by a plain
        `mkdir` in this process under the same umask, so the assertion is "whatever the
        operator's umask says", not a mode written down here.
        """
        plane = self.plane("committed")
        old = os.umask(0o022)
        try:
            control = plane / "control-dir"
            control.mkdir()
            proc = self.charter(plane, "persona", "remember", "steward", "a committed fact")
        finally:
            os.umask(old)
        self.assertEqual(proc.returncode, 0, f"{proc.stdout}\n{proc.stderr}")
        mem = plane / "personas" / "steward" / "memory"
        self.assertTrue(mem.is_dir(), "the committed memory directory was not created, so "
                                      "this case measures nothing")
        self.assertEqual(
            stat.S_IMODE(mem.stat().st_mode), stat.S_IMODE(control.stat().st_mode),
            "charter tightened a committed directory the operator's umask had modes for")
        self.assertNotEqual(stat.S_IMODE(mem.stat().st_mode) & 0o077, 0,
                            "under umask 022 a plain mkdir is 0755 — a 0700 here means "
                            "`mkdir_for` stopped dispatching and is privatising everything")

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


class TheDispatchOnWhereThePathIs(PersonaIso):
    """`config.mkdir_for` — the walk for a writer that is **handed** its directory.

    `private_mkdir` closes the writers that name a state path themselves. It cannot close
    the ones that do not: `memstore.write(mem_dir, …)` is handed
    ``personas/<n>/memory`` on one call and ``PERSONA_STATE_DIR/ephemeral/<sid>/<n>`` on
    the next, and only the caller knows which. Routing that one caller would have left the
    next handed writer exactly as exposed, so the dispatch is on **where the path is**, at
    runtime, and both directions of it are pinned here — a `mkdir_for` that privatised
    everything would pass the sweep above while turning committed persona directories 0700.
    """

    def test_a_handed_state_path_is_private_under_every_umask(self) -> None:
        for um in UMASKS:
            with self.subTest(umask=oct(um)):
                # Removed between rounds so each umask CREATES `.charter/` itself, which is
                # the flow from the issue — a fresh clone has none.
                shutil.rmtree(config.STATE_DIR, ignore_errors=True)
                leaf = config.STATE_DIR / f"handed-{um:03o}" / "a" / "b"
                old = os.umask(um)
                try:
                    config.mkdir_for(leaf)
                finally:
                    os.umask(old)
                chain = modes_up_to(leaf, config.STATE_DIR)
                self.assertGreaterEqual(len(chain), 4, "a short chain makes this vacuous")
                for p, mode in chain.items():
                    self.assertEqual(mode & 0o077, 0, f"{p} came out {oct(mode)[-3:]}")

    def test_a_handed_path_outside_it_keeps_the_umask_and_is_not_tightened(self) -> None:
        """Measured against a control directory made by a plain `mkdir` in this process
        under the same umask — so the assertion is "whatever the operator's umask says",
        not a mode written down here that a changed default would quietly falsify."""
        old = os.umask(0o022)
        try:
            control = Path(self.tmp) / "control"
            control.mkdir()
            d = config.PERSONAS_DIR / "steward" / "memory"
            config.mkdir_for(d)
        finally:
            os.umask(old)
        self.assertEqual(stat.S_IMODE(d.stat().st_mode),
                         stat.S_IMODE(control.stat().st_mode),
                         "charter tightened a committed directory it merely wrote into")
        self.assertNotEqual(stat.S_IMODE(d.stat().st_mode) & 0o077, 0,
                            "0700 here means the dispatch stopped dispatching")

    def test_the_state_directory_itself_counts_as_under_it(self) -> None:
        self.assertTrue(config.under_state(config.STATE_DIR))
        self.assertTrue(config.under_state(config.STATE_DIR / "vaults"))

    def test_a_sibling_sharing_its_name_as_a_prefix_does_not(self) -> None:
        """The classic prefix bug: ``.charter-backup`` starts with ``.charter`` and is a
        different directory. A ``startswith`` on the bare string says otherwise."""
        self.assertFalse(config.under_state(
            config.STATE_DIR.parent / f"{config.STATE_DIR.name}-backup"))

    def test_a_traversal_back_out_of_it_does_not(self) -> None:
        self.assertFalse(config.under_state(config.STATE_DIR / ".." / "personas" / "x"))

    def test_a_path_reaching_it_through_a_symlink_still_does(self) -> None:
        """The two spellings disagree here, and the answer is "yes".

        Lexically ``<tmp>/link/cache`` is nowhere near the state directory; resolved it is
        inside it. Built here rather than relying on the platform's own (``/tmp`` →
        ``/private/tmp`` on macOS) so the case is a case on every platform.
        """
        config.private_mkdir(config.STATE_DIR)
        link = Path(self.tmp) / "link-to-state"
        link.symlink_to(config.STATE_DIR)
        self.assertFalse(str(link / "cache").startswith(str(config.STATE_DIR)),
                         "precondition: the lexical spelling must NOT match, or this "
                         "case is not about resolution at all")
        self.assertTrue(config.under_state(link / "cache"))


class TheHandedWriterAsksWhereItIs(PersonaIso):
    """`memstore`, the writer the whole class of defect ran through, at handler level.

    One flow, two quadrants, one umask: `persona.remember` picks the directory and
    `memstore.write` creates it, and the mode has to come out different for the two.
    """

    def test_ephemeral_memory_is_private_and_committed_memory_is_left_alone(self) -> None:
        from charter import persona

        self.make_persona("steward")
        old = os.umask(0o022)
        try:
            control = Path(self.tmp) / "control"
            control.mkdir()
            # `make_persona` scaffolded the committed store already; remove it so the
            # committed half measures a directory THIS call creates, not that one.
            shutil.rmtree(persona.memory_dir("steward"), ignore_errors=True)
            eph = persona.remember("steward", "an ordinary scratch note", ephemeral=True)
            com = persona.remember("steward", "a fact worth committing")
        finally:
            os.umask(old)

        chain = modes_up_to(eph.parent, config.STATE_DIR)
        self.assertGreaterEqual(len(chain), 4, "a short chain makes this vacuous")
        for p, mode in chain.items():
            self.assertEqual(mode & 0o077, 0,
                             f"{p} came out {oct(mode)[-3:]} — the ephemeral quadrant is "
                             f"under `.charter/` and the umask decided its mode")
        self.assertEqual(stat.S_IMODE(com.parent.stat().st_mode),
                         stat.S_IMODE(control.stat().st_mode),
                         "the committed quadrant was tightened; it is the operator's")


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

    #: Every spelling of "make a directory here" the stdlib offers, and where each one
    #: keeps its path. The scan advertised ``makedirs`` coverage while reading the
    #: RECEIVER of every attribute call — which is where ``p.mkdir()`` keeps its path and
    #: not where ``os.makedirs(p)`` does, so ``os.makedirs`` was scanned as the expression
    #: ``os`` and never flagged. Keying on the NAME instead (``makedirs`` takes an
    #: argument, ``mkdir`` a receiver) swaps one spelling for another and still lets
    #: ``os.mkdir(p)`` through, so the table is the test: each of these creates a
    #: directory at a state path, and the scan owes the same answer to all of them.
    MAKERS = {
        "bound method, path is the receiver":
            "def f():\n    (config.STATE_DIR / 'x').mkdir(parents=True, exist_ok=True)\n",
        "module function, path is argument 0":
            "import os\ndef f():\n    os.makedirs(config.STATE_DIR / 'x', exist_ok=True)\n",
        "os.mkdir — the one a `makedirs` name-match still misses":
            "import os\ndef f():\n    os.mkdir(config.STATE_DIR / 'x')\n",
        "the path arrives by keyword":
            "import os\ndef f():\n    os.makedirs(name=config.STATE_DIR / 'x')\n",
        "imported bare":
            "from os import makedirs\ndef f():\n    makedirs(config.STATE_DIR / 'x')\n",
        "imported under another name":
            "from os import makedirs as md\ndef f():\n    md(config.STATE_DIR / 'x')\n",
        "unbound, path is argument 0 of an attribute call":
            "from pathlib import Path\ndef f():\n    Path.mkdir(config.STATE_DIR / 'x')\n",
    }

    def test_every_spelling_of_making_a_directory_is_scanned(self) -> None:
        """The scan's own title claims ``mkdir``/``makedirs``. This is that claim."""
        for label, src in self.MAKERS.items():
            with self.subTest(label):
                hits = scan.violations(src, self.names)
                self.assertEqual([e for _ln, e in hits], ["config.STATE_DIR / 'x'"],
                                 f"{label}: a state directory made here went unseen")

    def test_the_same_spellings_on_a_committed_path_are_left_alone(self) -> None:
        """The other half of the claim, and the one a scan that flagged every argument
        would fail: widening where the path is looked for must not widen WHAT counts as
        one, or the fix is to route committed directories through the private walk."""
        for label, src in self.MAKERS.items():
            with self.subTest(label):
                self.assertEqual(
                    scan.violations(src.replace("STATE_DIR", "PERSONAS_DIR"), self.names),
                    [], f"{label}: a committed directory was flagged as state")

    def test_a_mode_argument_is_not_mistaken_for_a_path(self) -> None:
        """`Path.mkdir`'s first positional argument is the MODE. Scanning every position
        that could hold a path means scanning that one too, and it must not hit."""
        src = "def f():\n    (config.PERSONAS_DIR / 'x').mkdir(0o700, exist_ok=True)\n"
        self.assertEqual(scan.violations(src, self.names), [])

    def test_the_handed_half_sees_the_module_function_spellings_too(self) -> None:
        """The handed scan reads the path out of the same place the named one does, so
        the blind spot was shared. `memstore` is the module that fell through once."""
        caller = ("from . import store\n\n"
                  "def go():\n    store.write(config.STATE_DIR / 'x', 'y')\n")
        for label, body in (("os.makedirs", "    os.makedirs(mem_dir, exist_ok=True)\n"),
                            ("os.mkdir", "    os.mkdir(mem_dir)\n")):
            with self.subTest(label):
                mods = scan.modules_from({
                    "store": "import os\n\ndef write(mem_dir, text):\n" + body,
                    "caller": caller})
                self.assertEqual(scan.handed_violations(mods, self.names),
                                 {"charter/store.py": [(4, "mem_dir")]})


class TheHandedScanSeesWhatItClaims(unittest.TestCase):
    """The second half of the scanner's accuracy, against a package built for the purpose.

    This half is the one that was missing, so it gets the same treatment the first got: a
    scan that has quietly stopped propagating arguments reports a clean package, and a
    scan that propagates everything reports a package that cannot be cleaned. Both
    failures are checked here, because only one of them is loud in production.
    """

    def setUp(self) -> None:
        self.names = scan.state_attribute_names()

    def hits(self, **sources) -> dict:
        return scan.handed_violations(scan.modules_from(sources), self.names)

    #: The callee every case below hands a directory to — `memstore.write`'s shape.
    STORE = "def write(mem_dir, text):\n    mem_dir.mkdir(parents=True, exist_ok=True)\n"

    def test_a_state_path_passed_as_an_argument_taints_the_parameter(self) -> None:
        """The defect, in eight lines: nothing in `store` spells a state name."""
        self.assertEqual(
            self.hits(store=self.STORE,
                      caller="from . import store\n\n"
                             "def go():\n    store.write(config.STATE_DIR / 'x', 'y')\n"),
            {"charter/store.py": [(2, "mem_dir")]})

    def test_a_committed_path_passed_the_same_way_is_not(self) -> None:
        """A scan that flagged every parameter would flag this one, and would be "fixed"
        by making committed directories private — which is the other defect."""
        self.assertEqual(
            self.hits(store=self.STORE,
                      caller="from . import store\n\n"
                             "def go():\n    store.write(config.PERSONAS_DIR / 'x', 'y')\n"),
            {})

    def test_the_routed_call_clears_it(self) -> None:
        """`config.mkdir_for` is not a method named ``mkdir``, so the taint stays and the
        violation goes — which is what "routed" has to mean for the handed half."""
        self.assertEqual(
            self.hits(store="def write(mem_dir, text):\n    config.mkdir_for(mem_dir)\n",
                      caller="from . import store\n\n"
                             "def go():\n    store.write(config.STATE_DIR / 'x', 'y')\n"),
            {})

    def test_a_state_path_helper_taints_it_too(self) -> None:
        """The real caller's shape: `persona.remember` passes ``ephemeral_dir(name)``,
        not ``config.PERSONA_STATE_DIR`` spelled out at the call site."""
        self.assertEqual(
            self.hits(store=self.STORE,
                      persona="def ephemeral_dir(n):\n    return config.STATE_DIR / 'e' / n\n",
                      caller="from . import persona, store\n\n"
                             "def go(n):\n    store.write(persona.ephemeral_dir(n), 'y')\n"),
            {"charter/store.py": [(2, "mem_dir")]})

    def test_the_taint_survives_another_hop(self) -> None:
        """Depth of the call chain is not a way out of the scan."""
        self.assertEqual(
            self.hits(store=self.STORE,
                      middle="from . import store\n\n"
                             "def save(d, t):\n    store.write(d, t)\n",
                      caller="from . import middle\n\n"
                             "def go():\n    middle.save(config.STATE_DIR / 'x', 'y')\n"),
            {"charter/store.py": [(2, "mem_dir")]})

    def test_a_keyword_argument_is_the_same_question(self) -> None:
        self.assertEqual(
            self.hits(store=self.STORE,
                      caller="from . import store\n\n"
                             "def go():\n"
                             "    store.write(text='y', mem_dir=config.STATE_DIR / 'x')\n"),
            {"charter/store.py": [(2, "mem_dir")]})

    def test_a_local_assignment_does_not_hide_the_argument(self) -> None:
        self.assertEqual(
            self.hits(store=self.STORE,
                      caller="from . import store\n\n"
                             "def go():\n    d = config.STATE_DIR / 'x'\n"
                             "    store.write(d, 'y')\n"),
            {"charter/store.py": [(2, "mem_dir")]})

    def test_a_helper_of_the_same_name_in_another_module_does_not_taint_it(self) -> None:
        """The spelling trap this scan is built to avoid, and the one a bare-name match
        walks straight into: ``_dir()`` exists in more than one module here, and only some
        of them return state. `dispatch._dir()` returns a committed ``personas/`` path —
        matched by name against `frame`'s, it would be flagged forever, and the only way to
        clear it would be to route a committed directory through the private walk.
        """
        self.assertEqual(
            self.hits(store=self.STORE,
                      framey="def _dir():\n    return config.STATE_DIR / 'f'\n",
                      dispatch="def _dir():\n    return config.PERSONAS_DIR / 'd'\n\n"
                               "def go():\n    pass\n",
                      caller="from . import dispatch, store\n\n"
                             "def go():\n    store.write(dispatch._dir(), 'y')\n"),
            {})

    def test_an_untainted_parameter_is_left_alone(self) -> None:
        """No caller passes state at all — the whole package is clean, and a scan that
        cannot say so is a scan nobody can act on."""
        self.assertEqual(self.hits(store=self.STORE), {})

    def test_config_own_walk_is_exempt_and_named_in_full(self) -> None:
        """`config`'s own ``mkdir`` calls ARE the routing — flagging them would be asking
        the guard to route through itself. Exempted by ``module.function``, so the same
        function name in another module is still scanned."""
        for qual in scan.THE_WALK:
            module, _, fn = qual.rpartition(".")
            self.assertTrue(hasattr(config, fn), f"{qual} no longer exists in `config`; "
                                                 "the exemption list is stale")
        self.assertEqual(
            self.hits(config="def mkdir_for(p):\n    p.mkdir(parents=True, exist_ok=True)\n",
                      caller="from . import config as c\n\n"
                             "def go():\n    c.mkdir_for(config.STATE_DIR / 'x')\n"),
            {})
        self.assertEqual(
            self.hits(other="def mkdir_for(p):\n    p.mkdir(parents=True, exist_ok=True)\n",
                      caller="from . import other\n\n"
                             "def go():\n    other.mkdir_for(config.STATE_DIR / 'x')\n"),
            {"charter/other.py": [(2, "p")]},
            "the exemption is keyed to the bare function name, so any module can opt out "
            "of the scan by naming a function `mkdir_for`")


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
