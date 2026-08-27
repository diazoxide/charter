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

**And the same sentence about the files (#505).** Everything above is about directories,
and the files inside them were written at ``0o777 & ~umask`` the whole time — harmless for
exactly as long as the directory above them was 0700, which is the one thing charter
deliberately does not guarantee. `TheFilesInItAreChartersToChooseToo` is therefore the
issue's own reproduction: a ``.charter/`` that **already existed at 0755**, three commands,
and ``st_mode`` read off every file underneath. `TheDispatchOnWhereTheFileIs` is
`config.write_for` / `open_for` / `touch_for` at the level the CLI cannot reach, including
the one place the two halves give different answers — a pre-existing loose *file* is
tightened where a pre-existing loose *directory* is not, because charter tightens what is
its own and reports what is not.

Every mode assertion in this file sets the umask **explicitly** and compares against a
control file or directory made by this process under the same umask. A case that inherited
the ambient umask would pass on a laptop at 022 and say something different on a CI runner
at 002, and neither run would be measuring independence from it.
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


class APlaneTheCliCanRunIn(unittest.TestCase):
    """One `charter init` plane, copied per case — the fixture, with no cases of its own.

    A subprocess rather than a handler call, because the umask is a property of the
    process and because the defect was in the *order commands run in* — which is a thing
    only the CLI actually has.

    No ``test_`` methods, so unittest never builds the template for this class itself; the
    two suites below inherit it. Extracted when #505 added the second (`.charter/`'s
    **files**, not its directories): building the plane twice would have doubled the
    slowest fixture in the suite, and copying the twenty lines would have been the usual
    way for two measurements of the same thing to drift.
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


class TheCliDecidesIt(APlaneTheCliCanRunIn):
    """The real binary, in a real plane, with no ``.charter/`` in it yet."""

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


class TheFilesInItAreChartersToChooseToo(APlaneTheCliCanRunIn):
    """#505: the same sentence about the **files**, in the plane where it matters.

    #470 settled the mode of every directory charter creates. The files inside them were
    still written at ``0o777 & ~umask``, and that was harmless only for as long as the
    directory above them was 0700 — which is exactly the case charter deliberately does
    **not** guarantee. So the plane here is the one from the issue's reproduction: a
    ``.charter/`` that **already existed**, at 0755, made by somebody's ``mkdir -p``.
    charter will not chmod it (`test_an_existing_loose_state_directory_is_left_exactly_as_it_is`
    above pins that, and this case re-pins it as a precondition), so every file charter
    writes into it is reachable by every account on the machine unless the file itself
    says otherwise.

    **Measured, not reasoned.** ``os.stat().st_mode`` over every regular file under
    ``.charter/`` after a run of the CLI, under three umasks. The control is a file
    written by *this* process under the same umask, so "the umask no longer decides it" is
    a comparison rather than a constant somebody typed — and a case that asserted 0600
    against a machine whose umask happened to be 077 would pass while proving nothing.
    """

    #: The three commands the issue reproduces with, each of which writes a different file
    #: through a different writer: the vault registry, the guard sighting, the trace log
    #: and the ephemeral persona store. Run in one plane, in this order, because what the
    #: issue exhibits is the *set* of files left behind rather than any one of them.
    FLOWS = (
        (("vault", "add", "devops", "--provider", "plain-file"), ""),
        (("persona", "remember", "steward", "an ordinary scratch note", "--ephemeral"), ""),
        (("hook", "pretooluse"),
         json.dumps({"session_id": "sess-505", "cwd": ".", "tool_name": "Bash",
                     "tool_input": {"command": "ls"}})),
    )

    def _run_the_issue(self, plane: Path) -> list[Path]:
        for args, stdin in self.FLOWS:
            proc = self.charter(plane, *args, stdin=stdin)
            self.assertNotIn("Traceback (most recent call last):",
                             (proc.stdout or "") + (proc.stderr or ""),
                             f"`charter {' '.join(args)}` crashed:\n{proc.stdout}\n{proc.stderr}")
        files = sorted(p for p in (plane / ".charter").rglob("*") if p.is_file())
        self.assertGreaterEqual(
            len(files), 4,
            f"only {len(files)} file(s) under `.charter/` — the flows this case names "
            f"stopped writing, so it is measuring nothing: {files}")
        return files

    def test_every_file_under_a_pre_existing_loose_state_directory_is_private(self) -> None:
        seen: dict[int, set] = {}
        for um in UMASKS:
            with self.subTest(umask=oct(um)):
                plane = self.plane(f"files-{um:03o}")
                sd = plane / ".charter"
                sd.mkdir()
                os.chmod(sd, 0o755)              # the pre-existing loose one, by hand
                old = os.umask(um)
                try:
                    control = plane / f"control-{um:03o}"
                    control.write_text("x")
                    files = self._run_the_issue(plane)
                finally:
                    os.umask(old)

                self.assertEqual(
                    stat.S_IMODE(sd.stat().st_mode), 0o755,
                    "precondition: charter must NOT have chmod-ed the directory it did "
                    "not create — if it did, this case stopped being about the files")
                self.assertEqual(
                    stat.S_IMODE(control.stat().st_mode), 0o666 & ~um,
                    "precondition: the umask must actually be in force in this process, "
                    "or 'the umask no longer decides it' is not being tested at all")
                for f in files:
                    self.assertEqual(
                        stat.S_IMODE(f.stat().st_mode) & 0o077, 0,
                        f"under umask {oct(um)}, {f.relative_to(plane)} came out "
                        f"{oct(stat.S_IMODE(f.stat().st_mode))[-3:]} inside a 0755 "
                        f"`.charter/` — another account on this machine can read it")
                seen[um] = {stat.S_IMODE(f.stat().st_mode) for f in files}
        self.assertEqual(
            len(set(map(frozenset, seen.values()))), 1,
            f"the umask still decides it: "
            f"{[(oct(u), sorted(map(oct, m))) for u, m in seen.items()]}")

    def test_the_charter_created_directories_have_not_regressed(self) -> None:
        """The control #470 owns, re-run here because a fix to the files is exactly the
        kind of change that reaches for a chmod and takes the directories with it."""
        plane = self.plane("dirs-control")
        old = os.umask(0o022)
        try:
            self._run_the_issue(plane)
        finally:
            os.umask(old)
        dirs = [p for p in (plane / ".charter").rglob("*") if p.is_dir()]
        self.assertGreaterEqual(len(dirs), 2, "no directories to measure")
        for d in [plane / ".charter", *dirs]:
            self.assertEqual(
                stat.S_IMODE(d.stat().st_mode) & 0o077, 0,
                f"{d.relative_to(plane)} came out "
                f"{oct(stat.S_IMODE(d.stat().st_mode))[-3:]} — #470 regressed")

    def test_a_committed_file_is_still_the_operators_to_mode(self) -> None:
        """The other half of the dispatch, and the one a `write_for` that privatised
        everything would fail: `charter persona remember` without ``--ephemeral`` writes
        into ``personas/<n>/memory/``, which is committed and belongs to the operator's
        umask. Measured against a control file written by this process, so the assertion
        is "whatever the umask says" rather than a mode written down here.
        """
        plane = self.plane("committed-file")
        old = os.umask(0o022)
        try:
            control = plane / "control.txt"
            control.write_text("x")
            proc = self.charter(plane, "persona", "remember", "steward", "a committed fact")
        finally:
            os.umask(old)
        self.assertEqual(proc.returncode, 0, f"{proc.stdout}\n{proc.stderr}")
        written = sorted((plane / "personas" / "steward" / "memory").glob("*.md"))
        self.assertTrue(written, "nothing was committed, so this case measures nothing")
        for f in written:
            self.assertEqual(
                stat.S_IMODE(f.stat().st_mode), stat.S_IMODE(control.stat().st_mode),
                f"charter tightened {f.name}, which is committed and the operator's")
        self.assertNotEqual(
            stat.S_IMODE(written[0].stat().st_mode) & 0o077, 0,
            "under umask 022 an ordinary file is 0644 — a 0600 here means `write_for` "
            "stopped dispatching and is privatising everything")


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


class TheDispatchOnWhereTheFileIs(PersonaIso):
    """`config.write_for` / `open_for` / `touch_for` — `mkdir_for` for an inode with
    contents (#505), at the level the CLI sweep cannot reach.

    Every case runs in a **pre-existing 0755** state directory, because that is the case
    the whole issue is about: charter will not chmod a directory it did not create, so if
    the files do not decide their own mode nothing does.
    """

    def setUp(self) -> None:
        super().setUp()
        self.sd = Path(config.STATE_DIR)
        self.sd.mkdir(parents=True, exist_ok=True)
        os.chmod(self.sd, 0o755)

    def control(self, um: int) -> Path:
        """A file this process wrote under the same umask — what "not tightened" means.

        A mode written down here would be a second thing to keep in step with the
        platform; a file written beside it cannot drift.
        """
        c = Path(self.tmp) / f"control-{um:03o}-{id(self):x}"
        c.write_text("x")
        return c

    def test_a_state_file_is_private_under_every_umask(self) -> None:
        for um in UMASKS:
            with self.subTest(umask=oct(um)):
                old = os.umask(um)
                try:
                    control = self.control(um)
                    for name, write in (
                        ("write.json", lambda p: config.write_for(p, "{}")),
                        ("append.log", lambda p: self._append(p, "line\n")),
                        ("marker", config.touch_for),
                    ):
                        p = self.sd / f"{um:03o}-{name}"
                        write(p)
                        self.assertEqual(
                            stat.S_IMODE(p.stat().st_mode) & 0o077, 0,
                            f"{name} came out {oct(stat.S_IMODE(p.stat().st_mode))[-3:]}")
                finally:
                    os.umask(old)
                self.assertEqual(stat.S_IMODE(control.stat().st_mode), 0o666 & ~um,
                                 "precondition: the umask was not actually in force, so "
                                 "this round proves nothing about independence from it")

    @staticmethod
    def _append(p: Path, text: str) -> None:
        with config.open_for(p, "a") as f:
            f.write(text)

    def test_a_file_outside_it_keeps_the_umask_and_is_not_tightened(self) -> None:
        """The half that makes this a dispatch: `memstore` writes a committed
        ``personas/<n>/memory`` file through the same call, and those are the operator's.
        A `write_for` that privatised everything passes every case above and fails here."""
        old = os.umask(0o022)
        try:
            control = self.control(0o022)
            p = config.PERSONAS_DIR / "steward" / "memory" / "a.md"
            config.mkdir_for(p.parent)
            config.write_for(p, "committed")
        finally:
            os.umask(old)
        self.assertEqual(stat.S_IMODE(p.stat().st_mode), stat.S_IMODE(control.stat().st_mode),
                         "charter tightened a committed file it merely wrote")
        self.assertNotEqual(stat.S_IMODE(p.stat().st_mode) & 0o077, 0,
                            "0600 here means the dispatch stopped dispatching")

    def test_a_pre_existing_loose_state_file_is_tightened(self) -> None:
        """The file charter did not create — and the one answer here that differs from the
        directories', on purpose.

        A directory under ``$CHARTER_HOME`` may be somebody's home or a team share that
        charter merely landed in, so charter names it and prints the ``chmod`` (#331). A
        file charter is putting its own bytes into is charter's whatever its history, and
        leaving the old mode on it is #437 verbatim — which is why
        `secrets.registry._write` and `secrets.plain_file._write_private` have settled the
        mode on the descriptor of a pre-existing file since then. This is the same
        discipline, not a second policy.
        """
        for mode in (0o644, 0o666, 0o604, 0o640):
            with self.subTest(existing=oct(mode)):
                p = self.sd / f"pre-{mode:03o}.json"
                p.write_text("old")
                os.chmod(p, mode)
                config.write_for(p, "new")
                self.assertEqual(p.read_text(), "new")
                self.assertEqual(stat.S_IMODE(p.stat().st_mode) & 0o077, 0,
                                 f"a file that was {oct(mode)[-3:]} kept it while charter "
                                 f"wrote this plane's state into it")

    def test_the_content_is_never_on_disk_at_the_loose_mode(self) -> None:
        """The ordering `_private_fd` exists for, measured rather than asserted in prose.

        ``open(p, "w")`` truncates as it opens, so a fix that wrote first and chmod-ed
        afterwards would leave the new content readable for the length of the write. The
        mode is read *from inside the write*, on the same inode, while the file object is
        open — the only moment at which "there is no window" is falsifiable.
        """
        p = self.sd / "window.json"
        p.write_text("old")
        os.chmod(p, 0o666)
        during = []
        with config.open_for(p, "w") as f:
            during.append(stat.S_IMODE(os.fstat(f.fileno()).st_mode))
            f.write("secret")
        self.assertEqual(during, [0o600],
                         f"the file was {oct(during[0])[-3:]} while charter was writing "
                         f"this plane's state into it")

    def test_it_does_not_chmod_the_directory_it_writes_into(self) -> None:
        """The overreach the directories' half refuses (#331), which a file writer reaching
        for `os.chmod` on a parent would reintroduce from the other side."""
        config.write_for(self.sd / "x.json", "{}")
        self.assertEqual(stat.S_IMODE(self.sd.stat().st_mode), 0o755,
                         "charter chmod-ed the state directory it did not create")

    def test_append_does_not_truncate(self) -> None:
        """`trace` and `memstore`'s index are appenders, and an `O_TRUNC` reached for on
        the way to a private mode would silently empty the trace log."""
        p = self.sd / "trace.jsonl"
        self._append(p, "one\n")
        self._append(p, "two\n")
        self.assertEqual(p.read_text(), "one\ntwo\n")

    def test_touch_keeps_an_existing_file_and_bumps_its_mtime(self) -> None:
        """`Path.touch`'s own contract — every caller here uses it as a marker, and one
        that truncated would be a different function wearing the name."""
        p = self.sd / "mark"
        p.write_text("payload")
        os.utime(p, (1, 1))
        config.touch_for(p)
        self.assertEqual(p.read_text(), "payload")
        self.assertGreater(p.stat().st_mtime, 1)
        self.assertEqual(stat.S_IMODE(p.stat().st_mode) & 0o077, 0)

    def test_a_state_path_reached_through_a_symlink_is_still_state(self) -> None:
        """The dispatch is `under_state`, which resolves — so the file half inherits the
        answer the directory half already argued for, rather than asking a second time."""
        link = Path(self.tmp) / "link-to-state"
        link.symlink_to(self.sd)
        old = os.umask(0o000)
        try:
            config.write_for(link / "through-a-link.json", "{}")
        finally:
            os.umask(old)
        self.assertEqual(
            stat.S_IMODE((self.sd / "through-a-link.json").stat().st_mode) & 0o077, 0)

    def test_the_mode_is_a_mask_not_a_value(self) -> None:
        """`STATE_FILE_MODE` is asserted through ``& 0o077`` everywhere above for the
        reason `base._OTHERS` is a mask: 0644 is the mode everybody pictures, and 0604,
        0620 and 0606 hand the file to another account just as completely. This is the
        one place the constant itself is named, so a change to it is a decision somebody
        made rather than a test quietly following along."""
        self.assertEqual(config.STATE_FILE_MODE, 0o600)

    def test_the_atomic_writers_temp_file_is_private_by_construction(self) -> None:
        """`inflight` and `toolgate` write through a temp file and `os.replace`, which
        carries the SOURCE's mode onto the destination — so the scan not looking at
        `os.replace` is only honest if the sources are already private.

        `toolgate`'s temp file is a state path the write scan sees. `inflight`'s comes
        from `tempfile.mkstemp`, whose 0600 is documented rather than incidental — pinned
        here because the claim is load-bearing and a change to it would be silent.
        """
        fd, name = tempfile.mkstemp(dir=self.tmp)
        os.close(fd)
        self.addCleanup(os.unlink, name)
        self.assertEqual(stat.S_IMODE(os.stat(name).st_mode) & 0o077, 0,
                         "mkstemp stopped creating at 0600; every atomic writer that "
                         "leans on it now leaks its destination's mode")


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


class TheWriteScanSeesWhatItClaims(unittest.TestCase):
    """The file half of the scanner's accuracy, against sources written for the purpose.

    Same treatment the directory half gets, and for the same reason: a scan that has
    quietly stopped seeing anything reports a clean package, which is the most comfortable
    way for a coverage test to become decorative.
    """

    def setUp(self) -> None:
        self.names = scan.state_attribute_names()

    #: Every spelling of "put bytes in a file here", and where each keeps its path. The
    #: table IS the test, for the reason `TheScanSeesWhatItClaims.MAKERS` is: keying on the
    #: shape ("``write_text`` takes a receiver, ``open`` an argument") trades one spelling
    #: for another, and the one nobody listed is the one that ships.
    WRITERS = {
        "bound method, path is the receiver":
            "def f():\n    (config.STATE_DIR / 'x').write_text('y')\n",
        "bytes, same shape":
            "def f():\n    (config.STATE_DIR / 'x').write_bytes(b'y')\n",
        "a marker file — no content, same mode":
            "def f():\n    (config.STATE_DIR / 'x').touch()\n",
        "builtin open, path is argument 0":
            "def f():\n    open(config.STATE_DIR / 'x', 'w').close()\n",
        "builtin open, appending":
            "def f():\n    open(config.STATE_DIR / 'x', 'a').close()\n",
        "builtin open, exclusive create":
            "def f():\n    open(config.STATE_DIR / 'x', 'x').close()\n",
        "builtin open, read-plus still writes":
            "def f():\n    open(config.STATE_DIR / 'x', 'r+').close()\n",
        "Path.open, path is the receiver":
            "def f():\n    (config.STATE_DIR / 'x').open('w').close()\n",
        "the mode arrives by keyword":
            "def f():\n    open(config.STATE_DIR / 'x', mode='w').close()\n",
        "the path arrives by keyword":
            "def f():\n    open(file=config.STATE_DIR / 'x', mode='w').close()\n",
        "os.open, path is argument 0":
            "import os\ndef f():\n"
            "    os.open(config.STATE_DIR / 'x', os.O_WRONLY | os.O_CREAT, 438)\n",
        "os.open by keyword":
            "import os\ndef f():\n    os.open(path=config.STATE_DIR / 'x', flags=os.O_CREAT)\n",
        "unbound, path is argument 0 of an attribute call":
            "from pathlib import Path\ndef f():\n"
            "    Path.write_text(config.STATE_DIR / 'x', 'y')\n",
    }

    def test_every_spelling_of_writing_a_file_is_scanned(self) -> None:
        for label, src in self.WRITERS.items():
            with self.subTest(label):
                hits = scan.write_violations(src, self.names)
                self.assertEqual([e for _ln, e in hits], ["config.STATE_DIR / 'x'"],
                                 f"{label}: a state file written here went unseen")

    def test_the_same_spellings_on_a_committed_path_are_left_alone(self) -> None:
        """The other half of the claim: widening where the path is looked for must not
        widen WHAT counts as one, or the fix is to privatise committed files."""
        for label, src in self.WRITERS.items():
            with self.subTest(label):
                self.assertEqual(
                    scan.write_violations(src.replace("STATE_DIR", "PERSONAS_DIR"),
                                          self.names),
                    [], f"{label}: a committed file was flagged as state")

    #: Reads of a state file, which are not sites at all. A scan that flagged these would
    #: be asking charter to route a read through a writer — which is not a thing — and the
    #: noise would take the real answers with it.
    READERS = {
        "open with no mode is `open`'s documented default of 'r'":
            "def f():\n    open(config.STATE_DIR / 'x').close()\n",
        "an explicit text read":
            "def f():\n    open(config.STATE_DIR / 'x', 'r').close()\n",
        "a binary read":
            "def f():\n    open(config.STATE_DIR / 'x', 'rb').close()\n",
        "Path.open with no mode":
            "def f():\n    (config.STATE_DIR / 'x').open().close()\n",
        "read_text is not a write":
            "def f():\n    (config.STATE_DIR / 'x').read_text()\n",
    }

    def test_reading_a_state_file_is_not_a_violation(self) -> None:
        for label, src in self.READERS.items():
            with self.subTest(label):
                self.assertEqual(scan.write_violations(src, self.names), [],
                                 f"{label}: a read was reported as an unrouted write")

    def test_a_mode_it_cannot_read_counts_as_a_write(self) -> None:
        """The safe direction, stated as a case. ``open(p, mode)`` with *mode* computed
        elsewhere cannot be judged here, and a false positive is loud where a skipped
        writer is the defect itself."""
        src = "def f(mode):\n    open(config.STATE_DIR / 'x', mode).close()\n"
        self.assertEqual([e for _ln, e in scan.write_violations(src, self.names)],
                         ["config.STATE_DIR / 'x'"])

    def test_the_routed_calls_are_not_flagged(self) -> None:
        for name in scan.ROUTED_WRITE:
            with self.subTest(name):
                src = f"def f():\n    config.{name}(config.STATE_DIR / 'x', 'y')\n"
                self.assertEqual(scan.write_violations(src, self.names), [])

    def test_the_routed_names_exist_in_config(self) -> None:
        """Asserted against `config` so a rename cannot leave the list stale — the same
        guard `ROUTED` gets for the directory half."""
        for name in scan.ROUTED_WRITE:
            self.assertTrue(callable(getattr(config, name, None)),
                            f"`config.{name}` no longer exists; ROUTED_WRITE is stale")
        for qual in scan.THE_WRITE_WALK:
            module, _, fn = qual.rpartition(".")
            self.assertTrue(hasattr(config, fn),
                            f"{qual} no longer exists in `config`; the exemption is stale")

    def test_settling_the_mode_on_the_descriptor_is_what_clears_an_os_open(self) -> None:
        """`secrets.plain_file`, `secrets.fingerprint` and `secrets.registry` open state
        files directly because each has a policy the dispatch does not have — two of them
        read the mode back and REFUSE. What makes them correct is the ``fchmod``, so that
        is what the scan asks for, rather than naming the three functions in a list.
        """
        leaks = ("import os\ndef f():\n"
                 "    fd = os.open(config.STATE_DIR / 'x', os.O_WRONLY | os.O_CREAT, 384)\n"
                 "    os.write(fd, b'y')\n")
        settles = ("import os\ndef f():\n"
                   "    fd = os.open(config.STATE_DIR / 'x', os.O_WRONLY | os.O_CREAT, 384)\n"
                   "    os.fchmod(fd, 384)\n"
                   "    os.write(fd, b'y')\n")
        self.assertEqual([e for _ln, e in scan.write_violations(leaks, self.names)],
                         ["config.STATE_DIR / 'x'"],
                         "`os.open`'s mode argument is ignored for an inode that already "
                         "exists (#437) — a creation mode is not a private file")
        self.assertEqual(scan.write_violations(settles, self.names), [])

    def test_one_hop_of_indirection_does_not_hide_it(self) -> None:
        """The shape most of the writers actually have: a module-level path helper, then
        a write in the function that uses it."""
        src = ("def _cache_file():\n    return config.STATE_DIR / 'cache' / 'x.json'\n\n"
               "def save():\n    f = _cache_file()\n"
               "    f.write_text('y')\n")
        self.assertEqual([ln for ln, _ in scan.write_violations(src, self.names)], [6])

    def test_the_module_alias_does_not_hide_it(self) -> None:
        src = "def f():\n    (_cfg.STATE_DIR / 'x').write_text('y')\n"
        self.assertEqual([ln for ln, _ in scan.write_violations(src, self.names)], [2])

    #: How the two writers this scan could not see were *bound*, rather than what they
    #: wrote. `_local_assigns` read single-Name assignments only, so a name that arrived
    #: by tuple-unpack or as a loop variable expanded to itself and named nothing —
    #: `persona.set_active` and `workspace._rename_active_pointers` were both invisible on
    #: that alone, and both wrote pointer files at the umask.
    BINDINGS = {
        "tuple unpack — which element is which is not decidable, so both carry it":
            "def _pointers():\n    return config.SESSIONS_DIR / 'a', config.TERMINALS_DIR / 'b'\n\n"
            "def go():\n    sf, tf = _pointers()\n    sf.write_text('y')\n",
        "a loop over the names that were just unpacked":
            "def _pointers():\n    return config.SESSIONS_DIR / 'a', config.TERMINALS_DIR / 'b'\n\n"
            "def go():\n    sf, tf = _pointers()\n"
            "    for f in (sf, tf):\n        f.write_text('y')\n",
        "a loop over what a state directory globs":
            "def go():\n    for f in config.SESSIONS_DIR.glob('*.workspace'):\n"
            "        f.write_text('y')\n",
        "a loop over the state directories themselves":
            "def go():\n    for d in (config.SESSIONS_DIR, config.TERMINALS_DIR):\n"
            "        (d / 'x').write_text('y')\n",
    }

    def test_how_the_name_was_bound_does_not_hide_the_write(self) -> None:
        for label, src in self.BINDINGS.items():
            with self.subTest(label):
                self.assertTrue(scan.write_violations(src, self.names),
                                f"{label}: the write went unseen")

    def test_the_same_bindings_on_a_committed_path_are_left_alone(self) -> None:
        """Widening how a name is followed must not widen what counts as state, or the
        remedy for the noise is to privatise the committed tree."""
        for label, src in self.BINDINGS.items():
            with self.subTest(label):
                committed = src.replace("SESSIONS_DIR", "PERSONAS_DIR") \
                               .replace("TERMINALS_DIR", "PERSONAS_DIR")
                self.assertEqual(scan.write_violations(committed, self.names), [],
                                 f"{label}: a committed file was flagged as state")

    def test_the_handed_half_is_the_same_question_about_files(self) -> None:
        """`memstore.write(mem_dir, …)` again, one inode-kind over: nothing in the callee
        spells a state name, and the file it writes was at the umask exactly as the
        directory used to be."""
        caller = ("from . import store\n\n"
                  "def go():\n    store.write(config.STATE_DIR / 'x', 'y')\n")
        for label, body in (
                ("write_text", "    (mem_dir / 'a').write_text(text)\n"),
                ("open for append", "    (mem_dir / 'a').open('a').close()\n"),
                ("touch", "    (mem_dir / 'a').touch()\n")):
            with self.subTest(label):
                mods = scan.modules_from({
                    "store": "def write(mem_dir, text):\n" + body, "caller": caller})
                self.assertEqual(scan.handed_write_violations(mods, self.names),
                                 {"charter/store.py": [(2, "mem_dir / 'a'")]})

    def test_a_committed_path_handed_the_same_way_is_not(self) -> None:
        mods = scan.modules_from({
            "store": "def write(mem_dir, text):\n    (mem_dir / 'a').write_text(text)\n",
            "caller": "from . import store\n\n"
                      "def go():\n    store.write(config.PERSONAS_DIR / 'x', 'y')\n"})
        self.assertEqual(scan.handed_write_violations(mods, self.names), {})

    def test_the_routed_call_clears_the_handed_half_too(self) -> None:
        mods = scan.modules_from({
            "store": "def write(mem_dir, text):\n"
                     "    config.write_for(mem_dir / 'a', text)\n",
            "caller": "from . import store\n\n"
                      "def go():\n    store.write(config.STATE_DIR / 'x', 'y')\n"})
        self.assertEqual(scan.handed_write_violations(mods, self.names), {})

    def test_config_own_writers_are_exempt_and_named_in_full(self) -> None:
        """`config`'s own `open`/`touch` ARE the routing — flagging them would be asking
        the guard to route through itself. Exempted by ``module.function``, so the same
        function name in another module is still scanned."""
        callee = "def write_for(p, t):\n    open(p, 'w').close()\n"
        caller = "from . import {alias}\n\ndef go():\n    {alias}.write_for(config.STATE_DIR / 'x', 'y')\n"
        self.assertEqual(
            scan.handed_write_violations(
                scan.modules_from({"config": callee,
                                   "caller": caller.format(alias="config")}), self.names),
            {})
        self.assertEqual(
            scan.handed_write_violations(
                scan.modules_from({"other": callee,
                                   "caller": caller.format(alias="other")}), self.names),
            {"charter/other.py": [(2, "p")]},
            "any module could opt out of the scan by naming a function `write_for`")


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

    #: The writers #505 did **not** close, by file and by the path expression at the site
    #: — never by line number, which two people editing the same module would invalidate
    #: without either of them touching a mode.
    #:
    #: Both are `persona.set_active`: the per-session and per-terminal persona pointers
    #: (``f``, bound by ``for f in (sf, tf)``) and the plane-wide ``active-persona``. They
    #: are three lines and they are out of this change only because two other branches
    #: were live in `charter/persona.py` when it landed, and an edit in the middle of them
    #: buys a conflict for no schedule. Filed as its own issue.
    #:
    #: This list is a **ratchet, not an allowance**. The assertion below is equality, so a
    #: writer that gets routed fails here as a stale entry and a new unrouted one fails as
    #: an unlisted leak — neither direction can happen quietly, and "add a line to make the
    #: test pass" is a diff a reviewer sees.
    NOT_ROUTED_YET = {"charter/persona.py": ["config.ACTIVE_PERSONA_FILE", "f"]}

    def test_no_write_in_the_package_can_leave_a_loose_state_file(self) -> None:
        found = {f: sorted(expr for _ln, expr in hits)
                 for f, hits in scan.scan_package_writes().items()}
        expected = {f: sorted(e) for f, e in self.NOT_ROUTED_YET.items()}
        self.assertEqual(
            found, expected,
            "a file under `.charter/` written without `config.write_for` / `open_for` / "
            "`touch_for` comes out at `0o777 & ~umask`, and the directory above it is not "
            "guaranteed to be 0700 — charter does not chmod a state directory it did not "
            "create (#331, #505). Route it. If it really is out of reach, move it into "
            "`NOT_ROUTED_YET` and say why there; if it is now routed, delete its entry.\n"
            f"  found:  {found}\n  listed: {expected}")

    def test_the_ratchet_names_files_that_exist(self) -> None:
        """The one way an equality assertion cannot notice a stale entry: the module gets
        renamed or deleted, and the list keeps naming a path nobody will look at again."""
        for f in self.NOT_ROUTED_YET:
            self.assertTrue((REPO_ROOT / f).is_file(),
                            f"NOT_ROUTED_YET names {f}, which is not a file any more")


if __name__ == "__main__":
    unittest.main()
