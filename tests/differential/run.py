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
    #: Set when the command is SUPPOSED to be refused. Both sides must then fail, with the
    #: same status — a scenario that expects a refusal and gets a success is a failure, and
    #: so is one that stops being refused on only one side.
    expect_failure: bool = False

    def rust_args(self) -> list[str]:
        return self.rust if self.rust is not None else self.python


# `--no-sync` on every Python write that takes it: `alpha` is a LIVE workspace, so Python
# charter would reactively `git commit` (and try to push) what it just wrote. That is
# workspace *syncing*, not a plane write, and it is not in M1.1 — so it is switched off rather
# than compared against nothing.
SCENARIOS = [
    Scenario(
        name="vision",
        plane="daily",
        python=["workspace", "vision", "Ship the widget, then retire it", "-w", "alpha"],
    ),
    Scenario(
        name="vision-on-a-fresh-workspace",
        plane="daily",
        python=["workspace", "vision", "First words", "-w", "beta"],
    ),
    Scenario(
        name="vision-with-non-ascii",
        plane="daily",
        # An em-dash and a `·`: the characters that separate a faithful writer from one that
        # merely produces valid UTF-8.
        python=["workspace", "vision", "Ship it — properly · no shortcuts", "-w", "alpha"],
    ),
    Scenario(
        name="remember",
        plane="daily",
        python=["workspace", "remember", "The importer drops rows over 4 MB", "-w", "alpha",
                "--no-sync"],
        rust=["workspace", "remember", "The importer drops rows over 4 MB", "-w", "alpha"],
    ),
    Scenario(
        name="remember-a-multi-line-fact",
        plane="daily",
        python=["workspace", "remember", "Retries are capped at 3\nand the 4th is dropped",
                "-w", "alpha", "--no-sync"],
        rust=["workspace", "remember", "Retries are capped at 3\nand the 4th is dropped",
              "-w", "alpha"],
    ),
    Scenario(
        name="remember-into-an-empty-journal",
        plane="daily",
        python=["workspace", "remember", "Nothing was here before", "-w", "beta", "--no-sync"],
        rust=["workspace", "remember", "Nothing was here before", "-w", "beta"],
    ),
    Scenario(
        name="todo-add",
        plane="daily",
        python=["ws", "todo", "Cut the 0.63 release", "-w", "alpha"],
    ),
    Scenario(
        name="todo-add-to-a-workspace-with-no-todo-store",
        plane="daily",
        # `beta` has no `todos/` at all, so this also covers scaffolding the index.
        python=["ws", "todo", "Delete the old importer", "-w", "beta"],
    ),
    Scenario(
        name="todo-done-by-bare-slug",
        plane="daily",
        python=["ws", "todo", "done", "review-the-rollout-plan", "-w", "alpha"],
    ),
    Scenario(
        name="todo-done-by-full-stem",
        plane="daily",
        python=["ws", "todo", "done", "20260302-091400-review-the-rollout-plan", "-w", "alpha"],
    ),
    # Found by an adversarial review of PR #21: each of these diverged, and three of them
    # lost or misplaced the operator's data.
    Scenario(
        name="vision-shown-not-erased-by-empty-text",
        plane="daily",
        # `if text:` in charter, so empty text SHOWS. This wrote an empty vision over a
        # committed, hand-edited file.
        python=["workspace", "vision", "", "-w", "alpha"],
        pins_the_clock=False,
    ),
    Scenario(
        name="vision-on-a-workspace-that-is-not-there",
        expect_failure=True,
        plane="daily",
        # Refused on both sides now. This used to scaffold `workspaces/nope/`, which is what
        # made a traversing `-w` silent.
        python=["workspace", "vision", "invented", "-w", "nope"],
    ),
    Scenario(
        name="vision-name-that-walks-out-of-the-plane",
        expect_failure=True,
        plane="daily",
        python=["workspace", "vision", "pwned", "-w", "../../outside/escaped"],
    ),
    Scenario(
        name="remember-a-body-of-only-separator-controls",
        expect_failure=True,
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
        name="remember-a-body-padded-with-separator-controls",
        plane="daily",
        python=["workspace", "remember", "\x1fPadded fact\x1f", "-w", "alpha", "--no-sync"],
        rust=["workspace", "remember", "\x1fPadded fact\x1f", "-w", "alpha"],
    ),
    Scenario(
        name="vision-with-a-trailing-separator-control",
        plane="daily",
        python=["workspace", "vision", "Ship it\x1f", "-w", "alpha"],
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

    def paths(root: Path) -> set[str]:
        found = set()
        for p in root.rglob("*"):
            if not p.is_file():
                continue
            rel = str(p.relative_to(root))
            # `.git` is the workspace's own repository, not the plane format. Python charter
            # commits into it on a LIVE workspace and the Rust side does not touch git at all
            # (that is M1.4), so it is out of this comparison by design, not by accident.
            if rel.split(os.sep)[0] == ".git" or f"{os.sep}.git{os.sep}" in f"{os.sep}{rel}":
                continue
            if any(rel == k or rel.startswith(f"{k}{os.sep}") for k in ignore):
                continue
            found.add(rel)
        return found

    def dirs(root: Path) -> set[str]:
        found = set()
        for p in root.rglob("*"):
            if not p.is_dir():
                continue
            rel = str(p.relative_to(root))
            if ".git" in p.relative_to(root).parts:
                continue
            if any(rel == k or rel.startswith(f"{k}{os.sep}") for k in ignore):
                continue
            found.add(rel)
        return found

    out: list[str] = []
    for missing in sorted(dirs(left) - dirs(right)):
        out.append(f"    directory only python made: {missing}")
    for added in sorted(dirs(right) - dirs(left)):
        out.append(f"    directory only rust made:   {added}")
    a, b = paths(left), paths(right)
    for rel in sorted(a - b):
        out.append(f"    only python wrote: {rel}")
    for rel in sorted(b - a):
        out.append(f"    only rust wrote:   {rel}")
    for rel in sorted(a & b):
        if not filecmp.cmp(left / rel, right / rel, shallow=False):
            out.append(f"    differs: {rel}")
            out.extend(_diff_file(left / rel, right / rel))
        if os.name == "posix":
            lmode = stat.S_IMODE((left / rel).stat().st_mode)
            rmode = stat.S_IMODE((right / rel).stat().st_mode)
            if lmode != rmode:
                out.append(f"    mode differs: {rel} — python {lmode:o}, rust {rmode:o}")
    return out


def _escaped(scratch: Path, side: str, root: Path) -> list[str]:
    """Anything written outside *root* — a containment bug, which `rglob` over the plane
    cannot see by construction."""
    out = []
    for path in sorted((scratch / side).rglob("*")):
        if path.is_dir():
            continue
        try:
            path.relative_to(root)
        except ValueError:
            # `home` and `pins` are the harness's own; everything else is an escape.
            rel = path.relative_to(scratch / side)
            if rel.parts[0] in {"home", "pins"}:
                continue
            out.append(f"    {side} WROTE OUTSIDE ITS PLANE: {rel}")
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

        py = _run([sys.executable, "-m", "charter", *scenario.python], py_root, py_home, py_pins)
        rust_argv = [str(binary), *scenario.rust_args()]
        if scenario.pins_the_clock:
            rust_argv += ["--now", NOW_NAIVE]
        rs = _run(rust_argv, rs_root, rs_home, rs_pins)

        problems: list[str] = []
        # A non-zero status is only a problem when the scenario did not ask for one, or when
        # the two sides disagree. Two identical refusals are a result, not a failure.
        if py.returncode != rs.returncode:
            problems.append(
                f"    exit status differs: python {py.returncode} "
                f"({py.stderr.strip()}), rust {rs.returncode} ({rs.stderr.strip()})"
            )
        elif scenario.expect_failure and py.returncode == 0:
            problems.append("    both sides SUCCEEDED, and this scenario expects a refusal")
        elif not scenario.expect_failure and py.returncode != 0:
            problems.append(
                f"    both sides failed ({py.returncode}) and the scenario does not say so — "
                f"python: {py.stderr.strip()}"
            )
        problems.extend(_diff_trees(py_root, rs_root, scenario.ignore))
        problems.extend(_escaped(scratch, "python", py_root))
        problems.extend(_escaped(scratch, "rust", rs_root))
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
