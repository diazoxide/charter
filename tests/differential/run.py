#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "charter-cp @ git+https://github.com/diazoxide/charter@50d31dc66835592ccea625bab8f5a0da313f2444",
#   "time-machine>=2.16",
# ]
# ///
"""Differential tests: the same command, run by both implementations, must leave the same plane.

Spec decision 14 (as amended): every module that moves to Rust passes a differential test —
the same fixture plane and the same input give the same output and the same resulting plane in
both implementations. This is that test for the plane writes.

Each scenario copies a fixture plane TWICE. Python charter runs one command against the first
copy; the Rust `charter` binary runs the same command against the second. Then:

- every file under either plane is compared, by path and byte for byte;
- so is the set of directories, so a directory one side creates and the other does not is seen;
- on POSIX, so is each file's mode, because charter writes 0600 where it means private;
- so is the exit status;
- and so is stdout — unless the scenario says `stdout=DIFFERS` with the reason, which is how a
  known gap is recorded rather than quietly skipped. A scenario that says nothing must match.

Neither side may write OUTSIDE its plane copy. Each copy is laid out under its own directory
with a sentinel tree beside it, and both are checked afterwards: a containment bug writes where
`rglob` over the plane cannot see it, so it has to be looked for on purpose.

    ./run.py                        # every scenario
    ./run.py --scenario vision      # one
    ./run.py --binary path/to/charter

The Rust binary defaults to `target/debug/charter`; build it first (`cargo build -p charter-cli`).

Python is a DEV DEPENDENCY OF THIS TEST and of nothing else — never of the app, the binary or an
installer (spec decision 15). It is pinned to the same charter commit that generated the fixture
planes, so a fixture and the oracle that checks against it can never drift apart.

Determinism: the clock, the hostname, the user and the timezone are pinned for the Python side
exactly as `tests/fixtures/planes/generate.py` pins them, and the Rust side is given the same
instant through `--now`. Without that, a memory's filename — which carries `%Y%m%d-%H%M%S` of the
LOCAL clock — would differ by the second the two processes happened to run in.
"""

from __future__ import annotations

import argparse
import filecmp
import os
import shutil
import stat
import subprocess
import sys
import tempfile
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
PLANES = REPO / "tests" / "fixtures" / "planes"

# The instant both sides write with. A fixed LOCAL naive time on the Python side and the same
# string through `--now` on the Rust side; TZ is pinned to UTC so "local" is one thing.
NOW = datetime(2026, 5, 4, 11, 32, 17, tzinfo=timezone.utc)
NOW_NAIVE = NOW.replace(tzinfo=None).isoformat()
#: The stamp a memory written at `NOW` carries in its filename.
NOW_NAIVE_STAMP = NOW.strftime("%Y%m%d-%H%M%S")

HOSTNAME = "fixture-host"
USER = "fixture"
SESSION = "fixture-session-1"

# What no environment variable can reach: charter reads the wall clock directly and puts
# `socket.gethostname()` into filenames. Same shim as the fixture generator's.
SITECUSTOMIZE = '''\
"""Pins the clock and the hostname for the differential run (see run.py)."""
import os
import socket

import time_machine

time_machine.travel(os.environ["FIXTURE_NOW"], tick=False).start()
socket.gethostname = lambda: os.environ["FIXTURE_HOST"]
'''


@dataclass
class Scenario:
    """One command, run by both implementations against the same starting plane."""

    name: str
    plane: str
    #: Argument list for Python charter.
    python: list[str]
    #: Argument list for the Rust binary; defaults to the same one.
    rust: list[str] | None = None
    #: Paths (relative to the plane) neither side is compared on, with the reason.
    ignore: dict[str, str] = field(default_factory=dict)
    #: Why the two stdouts are not expected to match yet. Empty means they must.
    stdout_differs: str = ""
    #: Whether the Rust side takes `--now`. A read command does not.
    pins_the_clock: bool = True
    #: Run against each plane copy before the command, for a starting state the fixtures
    #: cannot carry — a symlink, a mode, a file in the way.
    setup: "Callable[[Path], None] | None" = None
    #: What a refusal must SAY, on the Rust side, as a substring of stderr. Setting it is
    #: what declares the command refused. "Both exited non-zero" is not a test: a binary
    #: that panics on every input satisfies it, and one did — three containment scenarios
    #: reported `ok` against a shim that ran nothing at all.
    refusal: str = ""
    #: Why the two stderrs are not expected to match. charter's refusals are prose and the
    #: Rust CLI's are not, so a refusal scenario states its own shape via `refusal` instead.
    stderr_differs: str = ""

    def rust_args(self) -> list[str]:
        return self.rust if self.rust is not None else self.python


def _two_hop_out_of_the_plane(root: Path) -> None:
    """A link out of the plane, and a second link THROUGH it carrying `..`.

    `memory/jump -> <outside>/inner`, then the memory file charter is about to create ->
    `jump/../authorized_keys`. Folding `..` before resolving turns that into
    `authorized_keys` — dropping the component that leaves — so the path reads as contained
    while the write lands outside. `os.path.realpath` resolves left to right and refuses it,
    so this is a port divergence and belongs here.

    The link is placed at the exact name the write would choose, which is why the clock is
    pinned: the filename carries `%Y%m%d-%H%M%S` of it.
    """
    outside = root.parent / "outside"
    (outside / "inner").mkdir(parents=True, exist_ok=True)
    store = root / "workspaces" / "alpha" / "memory"
    store.mkdir(parents=True, exist_ok=True)
    jump = store / "jump"
    if not jump.exists():
        jump.symlink_to(outside / "inner")
    target = store / f"{NOW_NAIVE_STAMP}-ssh-rsa-aaaa-attacker-example-com.md"
    target.unlink(missing_ok=True)
    target.symlink_to("jump/../authorized_keys")


def _symlink_a_memory_index_out_of_the_plane(root: Path) -> None:
    """Point `workspaces/alpha/memory/MEMORY.md` at a file outside the plane."""
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (outside / "important").write_text("PRECIOUS OPERATOR DATA\n")
    index = root / "workspaces" / "alpha" / "memory" / "MEMORY.md"
    index.parent.mkdir(parents=True, exist_ok=True)
    index.unlink(missing_ok=True)
    index.symlink_to(outside / "important")


def _dangle_a_memory_index_out_of_the_plane(root: Path) -> None:
    """The same, pointing at a file that does not exist: `canonicalize` fails for a dangling
    link, which is what let one past."""
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    index = root / "workspaces" / "alpha" / "memory" / "MEMORY.md"
    index.parent.mkdir(parents=True, exist_ok=True)
    index.unlink(missing_ok=True)
    index.symlink_to(outside / "planted")


def _symlink_a_workspace_out_of_the_plane(root: Path) -> None:
    """Point `workspaces/escape` at a directory outside the plane, and leave a file there
    both implementations must refuse to touch."""
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (outside / "workspace.md").write_text("# untouched\n")
    link = root / "workspaces" / "escape"
    if not link.exists():
        link.symlink_to(outside)


# `--no-sync` on every Python write that takes it: `alpha` is a LIVE workspace, so Python
# charter would reactively `git commit` (and try to push) what it just wrote. That is
# workspace *syncing*, not a plane write, and it is not in M1.1 — so it is switched off rather
# than compared against nothing.
#: charter confirms every write on stderr — `✓ Vision set for 'alpha' → …` — and the Rust
#: CLI is silent. That is the command PRESENTATION layer, which M1.1 does not port: the
#: binary exists here to drive this comparison, and what M1.1 delivers is the plane read and
#: written. Recorded per scenario rather than excluded globally, so the day the output is
#: ported the harness says "drop the note" instead of quietly agreeing.
CONFIRMS_ON_STDERR = (
    "charter confirms a write on stderr and the Rust CLI is silent; porting the command "
    "output is M2's, not M1.1's. The plane each side leaves is what this compares."
)

SCENARIOS = [
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="vision",
        plane="daily",
        python=["workspace", "vision", "Ship the widget, then retire it", "-w", "alpha"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="vision-on-a-fresh-workspace",
        plane="daily",
        python=["workspace", "vision", "First words", "-w", "beta"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="vision-with-non-ascii",
        plane="daily",
        # An em-dash and a `·`: the characters that separate a faithful writer from one that
        # merely produces valid UTF-8.
        python=["workspace", "vision", "Ship it — properly · no shortcuts", "-w", "alpha"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="remember",
        plane="daily",
        python=["workspace", "remember", "The importer drops rows over 4 MB", "-w", "alpha",
                "--no-sync"],
        rust=["workspace", "remember", "The importer drops rows over 4 MB", "-w", "alpha"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="remember-a-multi-line-fact",
        plane="daily",
        python=["workspace", "remember", "Retries are capped at 3\nand the 4th is dropped",
                "-w", "alpha", "--no-sync"],
        rust=["workspace", "remember", "Retries are capped at 3\nand the 4th is dropped",
              "-w", "alpha"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="remember-into-an-empty-journal",
        plane="daily",
        python=["workspace", "remember", "Nothing was here before", "-w", "beta", "--no-sync"],
        rust=["workspace", "remember", "Nothing was here before", "-w", "beta"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="todo-add",
        plane="daily",
        python=["ws", "todo", "Cut the 0.63 release", "-w", "alpha"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="todo-add-to-a-workspace-with-no-todo-store",
        plane="daily",
        # `beta` has no `todos/` at all, so this also covers scaffolding the index.
        python=["ws", "todo", "Delete the old importer", "-w", "beta"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="todo-done-by-bare-slug",
        plane="daily",
        python=["ws", "todo", "done", "review-the-rollout-plan", "-w", "alpha"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="todo-done-by-full-stem",
        plane="daily",
        python=["ws", "todo", "done", "20260302-091400-review-the-rollout-plan", "-w", "alpha"],
    ),
    # Found by an adversarial review of PR #21: each of these diverged, and three of them
    # lost or misplaced the operator's data.
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="vision-shown-not-erased-by-empty-text",
        plane="daily",
        # `if text:` in charter, so empty text SHOWS. This wrote an empty vision over a
        # committed, hand-edited file.
        python=["workspace", "vision", "", "-w", "alpha"],
        pins_the_clock=False,
    ),
    Scenario(
        name="vision-on-a-workspace-that-is-not-there",
        refusal="no workspace 'nope'",
        plane="daily",
        # Refused on both sides now. This used to scaffold `workspaces/nope/`, which is what
        # made a traversing `-w` silent.
        python=["workspace", "vision", "invented", "-w", "nope"],
    ),
    Scenario(
        name="vision-name-that-walks-out-of-the-plane",
        refusal="no workspace '../../outside/escaped'",
        plane="daily",
        python=["workspace", "vision", "pwned", "-w", "../../outside/escaped"],
    ),
    Scenario(
        name="remember-a-body-of-only-separator-controls",
        refusal="empty memory",
        stderr_differs="charter#1135: Python exits through its crash handler with a "
        "traceback here, so the two stderrs cannot match. `refusal` pins what the Rust "
        "side must say.",
        plane="daily",
        # U+001F is whitespace to `str.strip()` and not to Rust's `trim`, so this wrote a
        # memory file where charter refuses one.
        python=["workspace", "remember", "\x1f", "-w", "alpha", "--no-sync"],
        rust=["workspace", "remember", "\x1f", "-w", "alpha"],
        ignore={
            ".charter/reports": "Python charter does not CATCH its own `ValueError: empty "
            "memory` here — it exits 1 through the crash handler, which drafts a bug report "
            "into the plane (charter#1135). Both sides refuse the write, which is what this "
            "scenario pins; the crash artifact is not part of it.",
        },
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="remember-a-body-padded-with-separator-controls",
        plane="daily",
        python=["workspace", "remember", "\x1fPadded fact\x1f", "-w", "alpha", "--no-sync"],
        rust=["workspace", "remember", "\x1fPadded fact\x1f", "-w", "alpha"],
    ),
    Scenario(
        name="vision-through-a-workspace-symlinked-out-of-the-plane",
        plane="daily",
        # The NAME is legal, so the name rule cannot see this. What redirects the write is a
        # committed symlink, which travels with the plane to every machine that clones it.
        setup=_symlink_a_workspace_out_of_the_plane,
        python=["workspace", "vision", "pwned through a link", "-w", "escape"],
        refusal="outside the directories",
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="vision-with-a-trailing-separator-control",
        plane="daily",
        python=["workspace", "vision", "Ship it\x1f", "-w", "alpha"],
    ),
    # The read commands. Their OUTPUT is the presentation layer M1.1 does not port, but a
    # read must not change the plane, and until these existed nothing checked that.
    Scenario(
        name="workspace-list-changes-nothing",
        plane="daily",
        python=["workspace", "list"],
        pins_the_clock=False,
        stdout_differs="charter prints a table — active workspace, mode, vision — and the "
        "Rust CLI prints one name per line. Both list the same workspaces, including "
        "`default`, which is listable whether or not its directory exists.",
    ),
    Scenario(
        name="todo-list-changes-nothing",
        plane="daily",
        python=["ws", "todo", "-w", "alpha"],
        pins_the_clock=False,
        stdout_differs="charter prints an indented row with an age column and an escaped "
        "title; the Rust CLI prints slug and title. Porting the row is M2's.",
    ),
    Scenario(
        name="todo-forget-with-a-traversing-slug-deletes-nothing",
        plane="daily",
        # charter #339: the slug is untrusted, and `remove_file` took a neighbour's file.
        python=["ws", "todo", "forget", "../../beta/workspace", "-w", "alpha"],
        refusal="is not the slug of one todo",
    ),
    Scenario(
        name="todo-forget-needs-a-slug",
        plane="daily",
        python=["ws", "todo", "forget", "-w", "alpha"],
        refusal="needs the slug",
    ),
    Scenario(
        name="todo-forget-by-a-real-slug",
        plane="daily",
        stderr_differs=CONFIRMS_ON_STDERR,
        python=["ws", "todo", "forget", "review-the-rollout-plan", "-w", "alpha"],
    ),
    Scenario(
        name="todo-done-with-a-traversing-slug-deletes-nothing",
        plane="daily",
        python=["ws", "todo", "done", "../../beta/workspace", "-w", "alpha"],
        refusal="no such todo",
    ),
    Scenario(
        name="remember-through-a-memory-index-linked-out-of-the-plane",
        plane="daily",
        setup=_symlink_a_memory_index_out_of_the_plane,
        python=["workspace", "remember", "A durable fact", "-w", "alpha", "--no-sync"],
        rust=["workspace", "remember", "A durable fact", "-w", "alpha"],
        refusal="outside the directories",
    ),
    Scenario(
        name="remember-through-two-hops-out-of-the-plane",
        plane="daily",
        setup=_two_hop_out_of_the_plane,
        python=["workspace", "remember", "ssh-rsa AAAA attacker@example.com", "-w", "alpha",
                "--no-sync"],
        rust=["workspace", "remember", "ssh-rsa AAAA attacker@example.com", "-w", "alpha"],
        refusal="outside the directories",
    ),
    Scenario(
        name="remember-through-a-dangling-memory-index-link",
        plane="daily",
        setup=_dangle_a_memory_index_out_of_the_plane,
        python=["workspace", "remember", "A durable fact", "-w", "alpha", "--no-sync"],
        rust=["workspace", "remember", "A durable fact", "-w", "alpha"],
        refusal="outside the directories",
    ),
    Scenario(
        name="a-duplicate-todo-is-refused",
        plane="daily",
        # `alpha` already has "Review the rollout plan". Duplicate INTENT is refused because
        # closing one of a near-identical pair leaves its twin looking outstanding.
        python=["ws", "todo", "Review the rollout plan", "-w", "alpha"],
        refusal="already on the list",
    ),
]


def _env(root: Path, home: Path, pins: Path) -> dict[str, str]:
    """A FRESH environment, never the caller's.

    charter resolves its plane from `$CHARTER_ROOT` before anything else, and every shell
    inside a real plane has one — inherited, it would point both implementations at the
    operator's own plane and every write would land there. `CHARTER_ROOT` is then set
    positively, so the plane is pinned rather than merely un-inherited.
    """
    return {
        "CHARTER_ROOT": str(root),
        # A curated PATH for the same reason the fixture generator curates one: charter writes
        # a different harness layer when `claude` is on PATH than when it is not.
        "PATH": "/usr/bin:/bin",
        "HOME": str(home),
        "PYTHONPATH": str(pins),
        "PYTHONDONTWRITEBYTECODE": "1",
        "TZ": "UTC",
        "LC_ALL": "C.UTF-8",
        "USER": USER,
        "LOGNAME": USER,
        "FIXTURE_NOW": NOW.isoformat(),
        "FIXTURE_HOST": HOSTNAME,
        "CHARTER_SESSION_ID": SESSION,
        "CHARTER_NO_BACKGROUND_CHECKS": "1",
        "GIT_AUTHOR_DATE": NOW.isoformat(),
        "GIT_COMMITTER_DATE": NOW.isoformat(),
        "TERM": "dumb",
        "NO_COLOR": "1",
    }


def _refuse_enclosing_plane(root: Path) -> None:
    """Stop before writing if some directory above the scratch plane is itself a plane."""
    for parent in root.resolve().parents:
        if (parent / "charter.toml").is_file():
            raise SystemExit(
                f"refusing to run: {parent} is a control plane, and charter would act on it "
                f"instead of the copy"
            )


def _lay_out(scratch: Path, plane: str, side: str) -> tuple[Path, Path, Path]:
    """A copy of fixture plane *plane* for one side, with a home and a pins directory."""
    root = scratch / side / "plane"
    home = scratch / side / "home"
    pins = scratch / side / "pins"
    root.parent.mkdir(parents=True, exist_ok=True)
    home.mkdir(parents=True, exist_ok=True)
    pins.mkdir(parents=True, exist_ok=True)
    (pins / "sitecustomize.py").write_text(SITECUSTOMIZE)
    _refuse_enclosing_plane(root)
    shutil.copytree(PLANES / plane, root)
    # The fixtures cannot carry an empty directory, so the generator records them beside the
    # plane. Restoring them matters: "no `todos/`" and "an empty `todos/`" are different
    # starting states, and charter's readers answer differently for each.
    listing = PLANES / f"{plane}.empty-dirs"
    if listing.exists():
        for rel in listing.read_text().split():
            (root / rel).mkdir(parents=True, exist_ok=True)
    return root, home, pins


def _run(argv: list[str], root: Path, home: Path, pins: Path) -> subprocess.CompletedProcess:
    return subprocess.run(
        argv, cwd=root, env=_env(root, home, pins), capture_output=True, text=True
    )


def _diff_trees(left: Path, right: Path, ignore: dict[str, str]) -> list[str]:
    """Every way the two planes differ, as lines to print. Empty means identical."""

    def nodes(root: Path) -> dict[str, str]:
        """Every entry under *root* by path, with WHAT IT IS.

        Built from `lstat`, never `is_file()`: that follows a link, so a symlink whose target
        has matching bytes was indistinguishable from a regular file — and the most
        security-relevant scenario here is about a symlink. A dangling link was in neither
        the file set nor the directory set, so it was invisible altogether.
        """
        found = {}
        for p in root.rglob("*"):
            rel = str(p.relative_to(root))
            if any(rel == k or rel.startswith(f"{k}{os.sep}") for k in ignore):
                continue
            if p.is_symlink():
                # Relative to the side's own directory: each copy lives under `python/` or
                # `rust/`, so an identical link reads as two different absolute paths.
                target = os.readlink(p)
                side = root.parent
                if os.path.isabs(target):
                    try:
                        target = f"<side>/{Path(target).relative_to(side)}"
                    except ValueError:
                        pass
                found[rel] = f"symlink -> {target}"
            elif p.is_dir():
                found[rel] = "dir"
            elif p.is_file():
                found[rel] = "file"
            else:
                found[rel] = "other"
        return found

    out: list[str] = []
    a, b = nodes(left), nodes(right)
    for rel in sorted(set(a) - set(b)):
        out.append(f"    only python has: {rel} ({a[rel]})")
    for rel in sorted(set(b) - set(a)):
        out.append(f"    only rust has:   {rel} ({b[rel]})")
    for rel in sorted(set(a) & set(b)):
        if a[rel] != b[rel]:
            out.append(f"    kind differs: {rel} — python {a[rel]}, rust {b[rel]}")
    # Only real files have bytes and a mode worth comparing.
    for rel in sorted(k for k in set(a) & set(b) if a[k] == "file" == b[k]):
        if not filecmp.cmp(left / rel, right / rel, shallow=False):
            out.append(f"    differs: {rel}")
            out.extend(_diff_file(left / rel, right / rel))
        if os.name == "posix":
            lmode = stat.S_IMODE((left / rel).stat().st_mode)
            rmode = stat.S_IMODE((right / rel).stat().st_mode)
            if lmode != rmode:
                out.append(f"    mode differs: {rel} — python {lmode:o}, rust {rmode:o}")
    return out


def _outside(scratch: Path, side: str, root: Path) -> dict[str, bytes]:
    """Every file beside the plane copy, with its contents.

    Taken before the command and again after, so what is reported is what the command
    WROTE — a scenario's own setup may legitimately put a file out here for the command to
    be refused against.
    """
    found = {}
    for path in sorted((scratch / side).rglob("*")):
        if path.is_dir():
            continue
        try:
            path.relative_to(root)
            continue  # inside the plane; the tree comparison covers it
        except ValueError:
            pass
        rel = path.relative_to(scratch / side)
        try:
            found[str(rel)] = path.read_bytes()
        except OSError:
            found[str(rel)] = b"<unreadable>"
    return found


def _escaped(side: str, before: dict[str, bytes], after: dict[str, bytes]) -> list[str]:
    """Anything the command wrote outside its plane copy — a containment bug, which `rglob`
    over the plane cannot see by construction."""
    out = []
    for rel in sorted(set(after) - set(before)):
        out.append(f"    {side} WROTE OUTSIDE ITS PLANE: {rel} (created)")
    for rel in sorted(set(before) & set(after)):
        if before[rel] != after[rel]:
            out.append(f"    {side} WROTE OUTSIDE ITS PLANE: {rel} (changed)")
    for rel in sorted(set(before) - set(after)):
        out.append(f"    {side} DELETED OUTSIDE ITS PLANE: {rel}")
    return out


def _diff_file(left: Path, right: Path) -> list[str]:
    """A few lines of context for a file that differs, or a note that it is not text."""
    try:
        a = left.read_text().splitlines()
        b = right.read_text().splitlines()
    except UnicodeDecodeError:
        return [f"      (binary; {left.stat().st_size} vs {right.stat().st_size} bytes)"]
    import difflib

    lines = list(difflib.unified_diff(a, b, "python", "rust", lineterm="", n=1))
    return [f"      {line}" for line in lines[:24]]


def check(scenario: Scenario, binary: Path) -> bool:
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        py_root, py_home, py_pins = _lay_out(scratch, scenario.plane, "python")
        rs_root, rs_home, rs_pins = _lay_out(scratch, scenario.plane, "rust")
        if scenario.setup is not None:
            scenario.setup(py_root)
            scenario.setup(rs_root)
        py_before = _outside(scratch, "python", py_root)
        rs_before = _outside(scratch, "rust", rs_root)

        py = _run([sys.executable, "-m", "charter", *scenario.python], py_root, py_home, py_pins)
        rust_argv = [str(binary), *scenario.rust_args()]
        if scenario.pins_the_clock:
            rust_argv += ["--now", NOW_NAIVE]
        rs = _run(rust_argv, rs_root, rs_home, rs_pins)

        problems: list[str] = []
        if py.returncode != rs.returncode:
            problems.append(
                f"    exit status differs: python {py.returncode} "
                f"({py.stderr.strip()}), rust {rs.returncode} ({rs.stderr.strip()})"
            )
        if scenario.refusal:
            if py.returncode == 0:
                problems.append("    this scenario expects a refusal and python SUCCEEDED")
            if scenario.refusal not in rs.stderr:
                problems.append(
                    f"    rust did not refuse with {scenario.refusal!r}; it said "
                    f"{rs.stderr.strip()!r}"
                )
        else:
            if py.returncode != 0:
                problems.append(
                    f"    python failed ({py.returncode}) and the scenario does not say so — "
                    f"{py.stderr.strip()}"
                )
            if scenario.stderr_differs:
                if py.stderr == rs.stderr:
                    problems.append(
                        "    stderr now MATCHES, but the scenario still says it differs "
                        f"({scenario.stderr_differs}) — drop the note"
                    )
            elif py.stderr != rs.stderr:
                problems.append("    stderr differs:")
                problems.append(f"      python {py.stderr!r}")
                problems.append(f"      rust   {rs.stderr!r}")
        problems.extend(_diff_trees(py_root, rs_root, scenario.ignore))
        problems.extend(
            _escaped("python", py_before, _outside(scratch, "python", py_root))
        )
        problems.extend(_escaped("rust", rs_before, _outside(scratch, "rust", rs_root)))
        if scenario.stdout_differs:
            if py.stdout == rs.stdout:
                problems.append(
                    "    stdout now MATCHES, but the scenario still says it differs "
                    f"({scenario.stdout_differs}) — drop the note"
                )
        elif py.stdout != rs.stdout:
            problems.append("    stdout differs:")
            problems.append(f"      python {py.stdout!r}")
            problems.append(f"      rust   {rs.stdout!r}")

        print(("ok   " if not problems else "DIFF ") + scenario.name)
        for line in problems:
            print(line)
        return not problems


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", type=Path, default=REPO / "target" / "debug" / "charter",
                    help="the Rust charter to test (default: target/debug/charter)")
    ap.add_argument("--scenario", action="append", default=None,
                    help="run only this scenario (repeatable)")
    args = ap.parse_args()

    if not args.binary.is_file():
        raise SystemExit(
            f"no Rust charter at {args.binary} — build it with `cargo build -p charter-cli`"
        )
    # A stale binary reports 17/17 for code that no longer exists. Cheap to notice: the
    # newest source file under `crates/` should not be newer than what is being tested.
    # Only what BUILDS the binary: a test file is newer all the time and says nothing about
    # whether the binary is current.
    newest = max(
        (
            f.stat().st_mtime
            for crate in (REPO / "crates").iterdir()
            if crate.is_dir()
            for f in (crate / "src").rglob("*.rs")
        ),
        default=0.0,
    )
    if newest > args.binary.stat().st_mtime:
        raise SystemExit(
            f"{args.binary} is older than the sources under crates/ — rebuild it with "
            f"`cargo build -p charter-cli`, or this run tests code that no longer exists"
        )

    wanted = SCENARIOS
    if args.scenario:
        names = {s.name for s in SCENARIOS}
        unknown = [n for n in args.scenario if n not in names]
        if unknown:
            ap.error(f"no such scenario: {', '.join(unknown)} (have {', '.join(sorted(names))})")
        wanted = [s for s in SCENARIOS if s.name in set(args.scenario)]

    failed = [s.name for s in wanted if not check(s, args.binary)]
    print()
    if failed:
        print(f"{len(failed)} of {len(wanted)} scenarios differ: {', '.join(failed)}")
        return 1
    print(f"all {len(wanted)} scenarios identical")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
