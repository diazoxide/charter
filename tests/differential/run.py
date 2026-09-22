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
- and so is stdout — unless the scenario says `stdout_differs` with the reason, which is how a
  known gap is recorded rather than quietly skipped, or `stdout_cut_at`, which is how a render
  that is ported as far as a declared boundary is compared up to it and no further. A scenario
  that says nothing must match.

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
import difflib
import filecmp
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
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


@dataclass(frozen=True)
class Divergence:
    """A difference between the two implementations that has been DECIDED, not one outstanding.

    Spec decision 15 requires every ported command to give the same result as the Python
    charter on the same input, and decision 17 freezes Python. A decision that changes what
    charter-app does therefore cannot be followed on the other side, and the difference is
    permanent. `stdout_differs` and `stderr_differs` are the wrong shape for it: both say "not
    yet", both pass the moment the two sides agree, and neither says what the other side does
    instead — a gap that closes itself is exactly what a DECIDED difference must not be
    allowed to look like.

    So this is the opposite of a waiver. It states, and the run checks, all of:

    - **`why`**, naming the record that decided it, because in six months the only thing
      standing between this and a bug nobody can explain is a sentence with an ADR number in
      it. A `why` that names no record is refused.
    - **both exit statuses**, separately. Not "they differ": the two numbers, each asserted,
      so a Rust side that started failing differently is as red as one that stopped failing.
    - **the Rust side's whole stderr**, byte for byte after the scenario's masks — the same
      bar `same_stderr` holds a ported refusal to. A divergence whose words are not pinned is
      a divergence nothing reads.
    - **a substring of the ORACLE's stderr**, so the run also fails when the Python side stops
      doing the thing this difference is a difference FROM. That direction is the one a
      reviewer forgets and the one that rots first.
    - **which top-level paths each side writes that the other does not**, and — this is the
      point — **every other path in the two planes is still compared byte for byte.** The
      divergence is fenced to exactly what it claims, so a second difference that crept in
      beside it is still a failure. Listed paths must be present on their own side and absent
      on the other, which is what makes the declaration fail loudly in BOTH directions: if the
      two sides ever agree again, the paths are no longer one-sided and this goes red.
    """

    #: Prose naming the record that decided it. Must cite an ADR and a spec decision.
    why: str
    python_exit: int
    rust_exit: int
    #: The Rust side's whole stderr, masks applied.
    rust_stderr: str
    #: A substring the ORACLE must still print, so the run sees Python changing too.
    python_stderr_has: str
    #: Top-level paths Python's plane has afterwards and the Rust side's does not.
    python_writes: tuple[str, ...] = ()
    #: The mirror: paths the Rust side leaves and Python does not.
    rust_writes: tuple[str, ...] = ()

    def one_sided(self) -> dict[str, str]:
        """The paths the tree comparison must skip — and nothing else may differ."""
        return {rel: self.why for rel in (*self.python_writes, *self.rust_writes)}


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
    #: The literal BOTH stdouts are cut at, when only the first part of a render is ported.
    #: Everything before that marker must match byte for byte; the marker and everything after
    #: it is each side's own.
    #:
    #: `stderr_cut_at`'s sibling, and NOT quite its twin, which is the difference to read
    #: carefully: there the Rust side stops early and its WHOLE stderr is compared against
    #: charter's prefix. Here both sides go on past the boundary and say different things below
    #: it — charter draws the rest of the plane, the Rust build names what it does not draw — so
    #: both are cut. That makes it weaker, so it is fenced twice: the marker must appear in
    #: BOTH outputs, and what is above it on charter's side must not be empty. Without the
    #: first, a Rust side that printed nothing at all would pass with an empty prefix on each
    #: side; without the second, a marker that turned out to be the first thing charter prints
    #: would compare nothing against nothing and report `ok` for any implementation at all.
    stdout_cut_at: str = ""
    #: Why that cut is where it is.
    stdout_cut_why: str = ""
    #: `(charter's words, charter-app's words, why)` triples: a DECIDED difference inside an
    #: otherwise byte-for-byte stdout. charter's words are replaced by charter-app's in
    #: charter's stdout, in order and once each, and the result must equal the Rust side's
    #: stdout exactly — so everything the triples do not name is still compared as it always
    #: was.
    #:
    #: `alert_rewrite`'s shape, widened from one row to a whole render, and held to the same
    #: rule: **both halves must be there**, charter's in charter's output and charter-app's in
    #: charter-app's, so a note cannot outlive the difference it records. It is `stdout_differs`
    #: turned inside out — that one says "not yet" about the whole render and passes the day
    #: the two agree; this one says what the difference IS and fails the day it stops being
    #: that. `DOCS_DIVERGE` is what fills it (charter-app#119).
    stdout_rewrite: list = field(default_factory=list)
    #: Whether the Rust side takes `--now`. A read command does not.
    pins_the_clock: bool = True
    #: Run against each plane copy before the command, for a starting state the fixtures
    #: cannot carry — a symlink, a mode, a file in the way.
    setup: "Callable[[Path], None] | None" = None
    #: Extra environment for BOTH sides, on top of `_env`. How a scenario drives the rungs of
    #: the two resolution ladders that live in the environment (`$CHARTER_WORKSPACE`,
    #: `$CHARTER_PERSONA`, the pane id the terminal pointer is keyed on). A value of `None`
    #: UNSETS the variable, which is how a scenario turns OFF a rung `_env` sets for every
    #: other one.
    env: dict[str, "str | None"] = field(default_factory=dict)
    #: Where the command runs, relative to the plane root; the default is the root itself.
    #: The tree you are standing in is a rung of the workspace ladder and the only one that
    #: cannot be planted as a file, so it has to be driven from here.
    cwd: str = ""
    #: What both sides are given on stdin. `charter statusline` is the whole reason this
    #: exists: its input is a JSON payload a harness pipes in, and a command run with no
    #: stdin of its own would inherit this harness's — which is a terminal under `./run.py`
    #: and a closed descriptor in CI, so the two would disagree about what "no payload"
    #: means depending on where the suite ran.
    stdin: str = ""
    #: What a refusal must SAY, on the Rust side, as a substring of stderr. Setting it is
    #: what declares the command refused. "Both exited non-zero" is not a test: a binary
    #: that panics on every input satisfies it, and one did — three containment scenarios
    #: reported `ok` against a shim that ran nothing at all.
    refusal: str = ""
    #: What a `PreToolUse` DENIAL must say, as a substring of the `permissionDecisionReason`
    #: — checked on BOTH sides' stdout.
    #:
    #: A hook refuses by PRINTING, not by exiting: the verdict is one JSON object on stdout
    #: and the status is 0 either way (`hooks.py:_deny`). So `refusal`, which reads stderr
    #: and declares the command failed, cannot say anything about one — and without this a
    #: guard scenario where NEITHER side fires is green while proving nothing, which is the
    #: `refusal` docstring's own objection one field over. Both sides are checked, so an arm
    #: that stops firing on the ORACLE goes red too.
    denies: str = ""
    #: The mirror, for the case that matters most: both sides must print NOTHING, which is
    #: how a `PreToolUse` hook says `allow`. A guard is only a guard if something gets
    #: through it, and "the canonical spelling still passes" is not a claim any denial
    #: scenario can make.
    allows: bool = False
    #: Why the two stderrs are not expected to match. charter's refusals are prose and the
    #: Rust CLI's are not, so a refusal scenario states its own shape via `refusal` instead.
    stderr_differs: str = ""
    #: The literal charter's stderr is CUT AT for this scenario, when only the first part
    #: of a command's output is ported. Everything before that marker must match byte for
    #: byte; the marker and everything after it is charter's alone.
    #:
    #: **Not a `startswith` test.** That was the first spelling and it was near-vacuous: it
    #: accepts any TRUNCATION of charter's output, so a Rust side that printed the table
    #: and dropped every refusal sentence — the whole reason a profile scenario exists —
    #: passed unchanged. An adversarial review proved it with two mutants. Cutting at a
    #: DECLARED boundary and comparing the rest exactly is the check that was meant.
    stderr_cut_at: str = ""
    #: Why that cut is where it is.
    stderr_cut_why: str = ""
    #: `(regex, why)` pairs whose matches are blanked on BOTH sides before stderr is
    #: compared. For a detail neither implementation writes: a TOML parser's diagnostic, an
    #: OS error string. The sentence around it still has to match byte for byte, which is
    #: the part charter wrote — so this hides a known quotation, never a difference of
    #: charter's own words.
    stderr_mask: list = field(default_factory=list)
    #: The same, for STDOUT. Separate from the field above rather than one list applied to
    #: both, because a mask is a claim about one stream: a pattern that is right for a
    #: refusal sentence is not automatically right for a render, and a guard's whole verdict
    #: is on stdout where every other scenario's words are on stderr.
    stdout_mask: list = field(default_factory=list)
    #: `(regex, why)` pairs naming ITEMS Python lists in `init`'s inventory that the Rust
    #: binary does not write, with the reason. Each matching item is taken out of Python's
    #: stderr before the comparison — out of a `+ item` line (and the headline's count with
    #: it) and out of a comma-separated `already present:` list — and everything else still
    #: has to match byte for byte. A pattern Python no longer prints fails the scenario, so
    #: the note cannot outlive the difference it records.
    python_only_items: list = field(default_factory=list)
    #: Paths beside the plane (relative to the side's directory) that PYTHON may write, with
    #: the reason. Only Python's: the Rust side writing any of them is still an escape.
    python_writes_outside: dict[str, str] = field(default_factory=dict)
    #: Path PREFIXES beside the plane that EITHER side may write, with the reason. For the one
    #: scenario shape where writing outside the plane copy is the command's whole job: a `save`
    #: pushes to a remote, and a remote is by definition not inside the tree being pushed. Each
    #: prefix must actually be written to by one of the sides, so a note cannot outlive the
    #: write it permits — otherwise a scenario could quietly stop pushing and still pass.
    writes_outside: dict[str, str] = field(default_factory=dict)
    #: `(regex, why)` pairs naming LINES the RUST side prints that charter does not. Each
    #: matching line is taken out of the Rust stderr before the comparison and everything else
    #: still has to match byte for byte. `python_only_items`' mirror, and held to the same rule:
    #: a pattern that matches nothing fails the scenario, so the note cannot outlive the line.
    rust_only_lines: list = field(default_factory=list)
    #: Exactly what those lines SAID, masks applied, as one block with a trailing newline.
    #:
    #: **Required whenever `rust_only_lines` is set**, and that requirement is the whole point.
    #: `rust_only_lines` is a set of regexes that DELETE; on its own it says "charter does not
    #: print this" and nothing at all about what charter-app printed instead. `save`'s
    #: directory breakdown — the block the 2026-09-12 incident's write-up calls the whole
    #: check, the one thing that would have shown a stray `uv.lock` going to `main` — was
    #: matched by `^• +\d+ {2}` and thrown away, so the only thing that had ever read its
    #: counts, its ordering or its directory names was a Rust unit test asserting that the
    #: Rust code does what the Rust code does. The differential is what compares two
    #: implementations; a line it deletes unread is outside it (M2.15).
    rust_only_block: str = ""
    #: What each side's plane must agree on that is not a file the tree comparison can read
    #: byte for byte — a clone's branch, commit and config, which live under a `.git` whose
    #: index and reflogs carry timestamps and inode numbers. Run against each plane after the
    #: command; the two answers must be equal. The `.git` itself is then `ignore`d, and this
    #: is what stands in for it rather than nothing.
    facts: "Callable[[Path], str] | None" = None

    #: How many ALERT LINES charter draws in this scenario's status line, when the scenario
    #: compares them — `statusline._alerts`, which charter-app ports as `alerts.rs`.
    #:
    #: The alert rows sit BELOW the zone rule, where `stdout_cut_at` stops comparing, and below
    #: zone 2, which charter-app does not draw. So they cannot be compared by position: they are
    #: picked out of both stdouts by [`ALERT_MARK`] and the two lists must be equal, line for
    #: line and byte for byte — frame, padding, colour and truncation included.
    #:
    #: **The number is asserted of charter's side**, and that is what keeps the comparison from
    #: going vacuous in either direction: a scenario written to produce a reinit row that
    #: produces none would otherwise compare two empty lists and report `ok`, and one that
    #: starts producing a second row nobody asked about is a change in the oracle worth seeing.
    alerts: "int | None" = None
    #: `(charter's words, charter-app's words, why)`: the one DECIDED difference in an alert
    #: row. charter's words are replaced by charter-app's in charter's lines before they are
    #: compared, and both halves are held to being there — charter's in charter's rows and
    #: charter-app's in charter-app's — so the note fails the day either side stops saying it.
    #: The `why` must cite an ADR, as a `Divergence` must.
    alert_rewrite: "tuple[str, str, str] | None" = None

    #: A refusal whose WORDS are ported too: stderr is then compared byte for byte, as it
    #: is for a success, rather than only searched for `refusal`.
    same_stderr: bool = False

    #: Literal strings NEITHER side may print, on stdout or on stderr.
    #:
    #: charter's rule for a credential is that it is refused **by KIND and never by the
    #: matched text** — `secretshape`'s module docstring, `handoff.rs`'s refusal and the
    #: comment over `A_BRIEF_WITH_A_SECRET` all say so, and until this field existed nothing
    #: checked it. A refusal that quoted the value would put the credential in the very
    #: transcript the refusal exists to keep it out of, and both implementations would have
    #: agreed about it, so `same_stderr` cannot see it either: two sides leaking the same
    #: secret match byte for byte.
    #:
    #: **Every needle must actually be in the input**, or the scenario fails as vacuous. A
    #: string that is not in the brief on stdin and not in a file of the plane the command
    #: is about to read is one neither side could have printed, and asserting its absence
    #: would be a test that passes against an implementation that prints nothing at all —
    #: the shape `stderr_cut_at`'s docstring already records this suite getting wrong once.
    never_says: tuple[str, ...] = ()

    #: Why this scenario's plane `origin` is a local path rather than a URL on a forge charter
    #: knows — see `_forge_trap` below, which is where the rest of this is explained.
    local_origin_why: str = ""

    #: A difference the two implementations are SUPPOSED to have, declared in full. See
    #: `Divergence`. It replaces the exit-status and stderr comparisons with its own, stricter
    #: ones; everything else about the scenario — stdout, the rest of the tree, and what may
    #: be written outside the plane — is checked exactly as it is for any other.
    diverges: "Divergence | None" = None

    def rust_args(self) -> list[str]:
        return self.rust if self.rust is not None else self.python


def _plant_a_todo_linked_out_of_the_plane(root: Path) -> None:
    """A committed `todos/leak.md -> <outside>/secret.md`, holding a heading and a body.

    The duplicate check reads every entry of the store to compare against. Gating only the
    DIRECTORY left each entry ungated, so this file was read, its heading echoed back by
    `ws todo`'s refusal, and the rest of it used as the comparison oracle — charter #442's
    shape on the one read path left. Python refuses to read it (`memstore.files` is the one
    gate), records the todo, and never mentions the outside file.
    """
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (outside / "secret.md").write_text(
        "# Board minutes: layoffs in Q3\n\n_2026-03-02 09:14 · persistent_\n\nCONFIDENTIAL\n"
    )
    store = root / "workspaces" / "alpha" / "todos"
    store.mkdir(parents=True, exist_ok=True)
    leak = store / "leak.md"
    leak.unlink(missing_ok=True)
    leak.symlink_to(outside / "secret.md")


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


def _a_workspace_layer_the_plane_has_moved_past(root: Path) -> None:
    """`beta`'s generated settings as an OLDER charter left them, with a marker vouching for
    exactly that text.

    The `stale` row, which is the one a wire is allowed to overwrite: charter's own file, still
    holding what charter's own record names, and the plane has since said something else. The
    fixture's copy is already current, so the drift has to be planted.
    """
    ws = root / "workspaces" / "beta"
    old = '{\n  "env": {\n    "CHARTER_HARNESS": "claude-code"\n  }\n}\n'
    (ws / ".claude").mkdir(parents=True, exist_ok=True)
    (ws / ".claude" / "settings.json").write_text(old)
    (ws / ".charter-generated").write_text(
        json.dumps(
            {".claude/settings.json": hashlib.sha256(old.encode("utf-8")).hexdigest()}, indent=2
        )
        + "\n"
    )


def _a_settings_file_the_operator_wrote(root: Path) -> None:
    """`beta`'s generated path, holding content no marker vouches for.

    The `foreign` row — the one charter must never repair. Both implementations have to leave
    the bytes alone AND say so, because a repair command that silently skips a file is one that
    reports the plane current while the plane's rules are out of force in it.
    """
    ws = root / "workspaces" / "beta"
    (ws / ".claude").mkdir(parents=True, exist_ok=True)
    (ws / ".claude" / "settings.json").write_text("# mine, and charter never wrote it\n")


def _a_workspace_missing_a_baseline_file(root: Path) -> None:
    """`beta` without its `refs/README.md` — what a workspace an older charter made looks
    like."""
    (root / "workspaces" / "beta" / "refs" / "README.md").unlink()


def _a_workspace_an_older_charter_stamped(root: Path) -> None:
    """`beta` with the whole baseline and an older structure stamp: the version bump alone."""
    (root / "workspaces" / "beta" / ".charter-structure").write_text("3\n")


def _a_generated_file_the_plane_stopped_declaring(root: Path) -> None:
    """The plane keeps no key that travels, so the file charter generated in each workspace is
    one nothing generates any more — the `removed` row.

    `hooks` stays, so the plane's settings still PARSE: a file charter cannot READ is the other
    case entirely, and there charter keeps the last good copy rather than withdrawing it.
    """
    (root / ".claude" / "settings.json").write_text('{"hooks":{"PreToolUse":[]}}')


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

def _declare_harness_profiles(root: Path) -> None:
    """A `charter.local.toml` in the plane, which no fixture can carry.

    The file is machine-local and gitignored by construction (ADR 0022), so a committed
    fixture holding one would be the very thing the feature refuses. It is written here, into
    each side's copy, so both read the same declarations.

    Every refusal class the parser has, in one file, because a refusal SENTENCE is what an
    operator acts on and a sentence that drifts between the two implementations is a plane
    that answers differently depending on which charter was asked.
    """
    (root / "charter.local.toml").write_text(
        '''[harness]
default = "claude-work"

[harness.claude-work]
kind = "claude"
command = ["claude", "--model", "opus"]
env = { CLAUDE_CONFIG_DIR = "~/.claude-work" }

[harness.claude]
kind = "claude"
command = ["~/.local/bin/claude"]

[harness.bad-kind]
kind = "opencodex"
command = ["x"]

# The spellings a refusal has to QUOTE BACK rather than merely refuse. Three of these were
# only in a Rust unit test, so the two implementations could disagree about how they read a
# wrong value back to the operator and nothing would say so.
[harness.kind-bool]
kind = true
command = ["claude"]

[harness.kind-array]
kind = ["claude"]
command = ["claude"]

[harness.kind-table]
kind = { a = 1 }
command = ["claude"]

[harness.kind-number]
kind = 7
command = ["claude"]

[harness.no-command]
kind = "codex"

[harness.shell-string]
kind = "codex"
command = "codex resume"

[harness.typo-env]
kind = "claude"
command = ["claude"]
enviroment = { CLAUDE_CONFIG_DIR = "~/.x" }

[harness.secret-env]
kind = "claude"
command = ["claude"]
env = { ANTHROPIC_API_KEY = "x" }

[harness.charter-env]
kind = "claude"
command = ["claude"]
env = { CHARTER_HARNESS = "claude-code" }

[harness.runs-charter]
kind = "claude"
command = ["charter", "status"]

[harness.save]
kind = "claude"
command = ["claude"]

[harness."has.dot"]
kind = "claude"
command = ["claude"]

[harness.parent.nested]
kind = "claude"
command = ["claude"]

[frame]
density = "wide"
''',
        encoding="utf-8",
    )


#: What `charter harness list` prints after its profile table, and this app does not.
#: The registry section is the harness REGISTRY's surface — every kind's capability deficits
#: as prose, opencode's and Codex's included — and porting it is not what M1.2 is. What the
#: prefix comparison still proves is the whole of the part that IS ported: the table, its
#: column widths, every refusal sentence, and the git-state fix line.
#: Where charter's `harness list` stops being about profiles: a blank line, then the harness
#: REGISTRY and its per-kind deficits as prose. The app ports profiles, not the registry.
HARNESS_LIST_REGISTRY_CUT = "\n\u2022 "
HARNESS_LIST_REGISTRY_SECTION = (
    "charter follows the profile table with the harness registry and its per-kind deficits; "
    "the app ports profiles, not the registry."
)

def _declare_a_profile_per_command_word(root: Path) -> None:
    """One profile named after every `charter <word>`, taken from the ORACLE's own list.

    `profiles.rs` carries that list as a hand-copied constant, and nothing tied the two
    together: the next command charter adds would silently become a name the Rust side
    accepts and the Python side refuses. Generating the fixture from `cli.command_words()`
    is the tie — a word only Python knows about shows up here as a profile Rust lists and
    Python refuses, and the scenario goes red.
    """
    # Asked of the ORACLE as a subprocess, under the same interpreter the scenarios run it
    # with, rather than imported here: this file must keep working whether or not charter is
    # importable in its own process.
    said = subprocess.run(
        [sys.executable, "-c",
         "from charter import cli; print('\\n'.join(sorted(cli.command_words())))"],
        capture_output=True, text=True, check=True,
    )
    lines = []
    for word in said.stdout.split():
        # The names charter's own validator refuses for other reasons are not this
        # scenario's subject; it is about the clash rule alone.
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_-]*", word):
            continue
        lines.append(f'[harness."{word}"]\nkind = "claude"\ncommand = ["claude"]\n')
    (root / "charter.local.toml").write_text("\n".join(lines), encoding="utf-8")


def _malform_the_local_file(root: Path) -> None:
    """A `charter.local.toml` that will not parse — the likeliest real state of the file,
    and one no other scenario reaches."""
    (root / "charter.local.toml").write_text("[harness\n", encoding="utf-8")


#: What a TOML parser says, which is its own words in both implementations.
TOML_DIAGNOSTIC = (
    r"could not be read \(.*?\), so no declared profile",
    "tomllib's diagnostic against toml_edit's; the sentence around it must still match",
)

def _python_only(stderr: str, items: list) -> tuple[str, list[str]]:
    """Python's stderr with the items `python_only_items` names taken out, and each pattern
    that matched nothing — the caller reports those, because the difference is gone."""
    patterns = [re.compile(pattern) for pattern, _why in items]
    hit = [False] * len(patterns)

    def python_only(item: str) -> bool:
        for i, pattern in enumerate(patterns):
            if pattern.fullmatch(item):
                hit[i] = True
                return True
        return False

    out, dropped = [], 0
    for line in stderr.splitlines(keepends=True):
        body = line.rstrip("\n")
        added = re.fullmatch(r"•   \+ (.*)", body)
        if added and python_only(added.group(1)):
            dropped += 1
            continue
        listed = re.fullmatch(r"(•   already present: )(.*)", body)
        if listed:
            kept = [i for i in listed.group(2).split(", ") if not python_only(i)]
            if not kept:
                continue
            line = f"{listed.group(1)}{', '.join(kept)}\n"
        out.append(line)
    text = "".join(out)
    if dropped:
        text = re.sub(r"— (\d+) item\(s\) written\.",
                      lambda m: f"— {int(m.group(1)) - dropped} item(s) written.", text,
                      count=1)
    return text, [items[i][0] for i, h in enumerate(hit) if not h]


def _rust_only(stderr: str, rules: list) -> tuple[str, str, list[str]]:
    """*stderr* without the lines the Rust side alone prints, THOSE LINES, and the patterns
    that matched nothing.

    Whole LINES, not substrings: what the Rust `save` adds is a block of its own — the
    directory breakdown of what is about to be committed — and removing it has to leave every
    sentence charter also writes exactly as it was.

    The removed lines are returned rather than dropped on the floor, because something has to
    read them: see `Scenario.rust_only_block`. In their original order, so the block's
    ordering — biggest directory first, ties by name — is part of what is compared.
    """
    kept: list[str] = []
    taken: list[str] = []
    seen = {pattern: False for pattern, _why in rules}
    for line in stderr.splitlines(keepends=True):
        for pattern, _why in rules:
            if re.search(pattern, line):
                seen[pattern] = True
                taken.append(line)
                break
        else:
            kept.append(line)
    return "".join(kept), "".join(taken), [p for p, hit in seen.items() if not hit]


def _opencode_already_installed(root: Path) -> None:
    """opencode's plugin, command and instructions in this side's `~/.config/opencode`, put
    there by the ORACLE's own writer before the command runs.

    Python's `init` writes them there on any machine that lacks them, and the Rust binary
    writes nothing outside the plane: charter-app v1 does not start opencode, and a shim whose
    hooks reach a binary that refuses opencode's tool hooks would block opencode everywhere.
    Installing them first is how the comparison sees everything else: Python then finds its
    shim current and says so in one `already present` item, which the scenario names in
    `python_only_items`. Both homes get them, so the Rust side runs on the same machine.
    """
    home = root.parent / "home"
    subprocess.run(
        [sys.executable, "-c",
         "from pathlib import Path; from charter.harness.opencode import OpenCodeHarness; "
         "OpenCodeHarness().wire(Path('.'))"],
        cwd=home, env={"HOME": str(home), "PATH": "/usr/bin:/bin"}, check=True,
        capture_output=True,
    )


#: The one item Python's `init` lists that the Rust binary never writes.
OPENCODE_SHIM = (
    r"opencode plugin/charter\.ts",
    "Python keeps opencode's global plugin current from `init`; charter-app writes nothing "
    "outside the plane and does not start opencode in v1",
)


#: The session context Python regenerates into opencode's global folder on every `init` and
#: `reinit` ("always overwrites": it is derived from the plane it was run in).
OPENCODE_CONTEXT = "home/.config/opencode/charter-context.md"
OPENCODE_CONTEXT_WHY = (
    "Python rewrites opencode's global session context from every `init`/`reinit`; "
    "charter-app writes nothing outside the plane"
)


def _in_a_directory_with_its_own_gitignore(root: Path) -> None:
    """A repository's own `.gitignore`, holding one line `init` wants and not the rest, a
    comment, a blank line, and no trailing newline."""
    (root / ".gitignore").write_text("node_modules/\n\n# local\n/.charter/\n*.log")


def _with_a_settings_file_of_its_own(root: Path) -> None:
    """The operator's `.claude/settings.json`: four-space indented, holding keys `init` has
    no business with — a float spelled as Python would not spell it, non-ASCII, a hook on
    another event, an ask rule already there — and no trailing newline."""
    (root / ".claude").mkdir(parents=True, exist_ok=True)
    (root / ".claude" / "settings.json").write_text(
        '{\n    "permissions": {\n        "allow": ["Bash(ls *)"],\n        "ask": '
        '["Bash(git push *)"]\n    },\n    "model": "opus",\n    "cleanupPeriodDays": 1E1,\n'
        '    "statusLine": {"type": "command", "command": "echo été"},\n'
        '    "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "true"}]}]}\n}'
    )


def _with_compact_settings(root: Path) -> None:
    """One line and no spaces, a trailing newline, an `env` whose harness value is blank, and
    an empty `PreToolUse`."""
    (root / ".claude").mkdir(parents=True, exist_ok=True)
    (root / ".claude" / "settings.json").write_text(
        '{"env":{"CHARTER_HARNESS":"","OTHER":"1"},"hooks":{"PreToolUse":[]}}\n'
    )


def _with_settings_claude_code_cannot_read(root: Path) -> None:
    """`NaN` is JSON to Python and not to Claude Code, so charter must not write it back."""
    (root / ".claude").mkdir(parents=True, exist_ok=True)
    (root / ".claude" / "settings.json").write_text('{"cleanupPeriodDays": NaN}\n')


def _with_an_opencode_json_that_allows_handoff(root: Path) -> None:
    (root / "opencode.json").write_text(
        '{"$schema": "https://opencode.ai/config.json", "permission": {"bash": '
        '{"charter handoff *": "allow", "git *": "ask"}}}'
    )


def _with_a_file_where_inventory_goes(root: Path) -> None:
    (root / "inventory").write_text("not a directory\n")


def _with_a_persona_of_its_own(root: Path) -> None:
    """A roster already: `init` scaffolds no front door over it."""
    (root / "personas" / "ops").mkdir(parents=True, exist_ok=True)
    (root / "personas" / "ops" / "persona.md").write_text("---\nname: ops\n---\n")


def _from_a_newer_charter(root: Path) -> None:
    (root / "charter.toml").write_text("schema = 2\n")


def _without_inventory(root: Path) -> None:
    shutil.rmtree(root / "inventory")


def _without_the_profiles_ignore_line(root: Path) -> None:
    ignore = root / ".gitignore"
    ignore.write_text(ignore.read_text().replace("/charter.local.toml\n", ""))


def _in_a_git_repository(root: Path) -> None:
    """The directory `init` is pointed at is the TOP LEVEL of a git working tree, with an
    `origin` on a forge charter knows.

    The equality case `commands._is_repo_top_level` is about — one person standing in one
    project — and the case ADR 0035 changed the default for. The origin is what names the
    repo on both sides (`_first_clone_name` takes the basename of the URL, not of the
    directory, and the directory here is called `plane` on purpose so a port that used the
    wrong one would say `'plane'`).

    No commit and no identity: `rev-parse --show-toplevel` and `remote get-url origin` are
    the only two git calls either side makes here, and both answer on an empty repository.
    `git init` plus `remote add` is byte-identical between two directories, so the `.git`
    trees are compared like any other file — which is how "neither side wrote into the
    repository's own internals" gets checked rather than assumed.
    """
    for args in (
        ["init", "-q", "-b", "main", "."],
        ["remote", "add", "origin", "https://github.com/acme/widget.git"],
    ):
        subprocess.run(
            ["git", *args], cwd=root, check=True, capture_output=True,
            env={"HOME": str(root.parent / "home"), "PATH": "/usr/bin:/bin",
                 "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"},
        )


def _both(*steps):
    def setup(root: Path) -> None:
        for step in steps:
            step(root)
    return setup


INIT = ["init", "--forge", "github", "--owner", "acme"]


def _init(name: str, *, python=INIT, rust=None, plane="", setup=None, refusal="",
          diverges=None, local_origin_why="") -> Scenario:
    """An `init` or `reinit` scenario: no clock, opencode's global files installed first, and
    the whole tree each side leaves compared."""
    return Scenario(
        name=name,
        plane=plane,
        python=python,
        rust=rust,
        pins_the_clock=False,
        setup=_both(_opencode_already_installed, *([setup] if setup else [])),
        refusal=refusal,
        # Python's stderr is not compared word for word when a divergence is declared — the
        # declaration pins what each side says instead — so deleting a line from it would
        # only hide what the oracle did.
        python_only_items=(
            [] if refusal or diverges or python[0] == "reinit" else [OPENCODE_SHIM]
        ),
        python_writes_outside={OPENCODE_CONTEXT: OPENCODE_CONTEXT_WHY},
        diverges=diverges,
        local_origin_why=local_origin_why,
    )


#: Everything Python's `init` leaves in a repository it was pointed at, at the top level —
#: the scaffolding ADR 0035 decided must not arrive unasked. `.charter/` is not among them:
#: `init` writes the ignore rule for it and not the directory.
COLONISED = ("charter.toml", ".claude", ".gitignore", "inventory", "opencode.json",
             "personas", "workspaces")

#: What charter-app says instead, byte for byte.
NOT_COLONISED = """\
✗ this is the git repo 'widget', and `charter init` does not make a repository into a control plane unless you ask it to. Nothing was written.
• A plane is a directory of its own, and this repo is the first clone in it:
      mkdir ../widget-plane && cd ../widget-plane
      charter init --forge github --owner acme
      charter discover && charter clone widget
  Nothing in this repo is read or written by any of that.
• To make THIS repo the plane instead — charter's own plane is one, which is why the option is here — ask for it by name:
      charter init --plane-is-this-repo --forge github --owner acme
  That writes charter.toml, .claude/settings.json, opencode.json, personas/, inventory/, workspaces/ into this repo, and charter's own rules into its tracked .gitignore.
• Why the default changed: docs/adr/0035-a-plane-is-untrusted-until-the-operator-opens-it.md, and charter-app spec decision 27. `charter init` anywhere that is not the top of a git repo is unchanged.
"""

#: The FIRST declared hole in spec decision 15's byte-for-byte guarantee. Read `Divergence`
#: before changing anything here, and ADR 0035 before deciding it should not exist.
INIT_IN_A_REPO_DIVERGES = Divergence(
    why=(
        "ADR 0035 reversed `charter init`'s default inside an existing repository — the plane "
        "goes in a directory of its own and the repo becomes its first clone, because ADR 0033 "
        "made 'which plane' a directory picked in a file dialog and a directory picked from a "
        "list has nobody standing in it. charter-app spec decision 27 carries it, and names it "
        "as a deliberate divergence from spec decision 15's byte-for-byte guarantee; the Python "
        "charter is frozen (decision 17) and keeps the old default. `--plane-is-this-repo` is "
        "the old behaviour, and the scenario beside this one holds the two byte for byte on it."
    ),
    python_exit=0,
    rust_exit=1,
    rust_stderr=NOT_COLONISED,
    python_stderr_has="✓ Initialized control plane (schema 1) — 8 item(s) written.",
    python_writes=COLONISED,
)


#: `init` and `reinit`, against an empty directory (`plane=""`) or a fixture.
INIT_SCENARIOS = [
    _init("init-into-an-empty-directory"),
    _init("init-with-no-owner-and-the-default-forge", python=["init"]),
    _init("init-with-no-front-door", python=[*INIT, "--no-front-door"]),
    _init("init-with-a-front-door-of-another-name",
          python=[*INIT, "--front-door", "front-door.2x", "--host", "git.example.com"]),
    _init("init-with-a-front-door-name-that-is-not-a-persona",
          python=[*INIT, "--front-door", "Bad Name"]),
    # charter warns `--front-door {name!r} is not a valid persona name`, so this compares
    # `repr()` itself, byte for byte, over the characters the ports disagreed about: U+00A0
    # (`Zs`), U+200B (`Cf`) and U+0890 (`Cf`, assigned after one of the hand-written tables
    # was pasted). The Rust side had three `repr`s and the one `init` used escaped none of
    # them — it escaped `char::is_control` and stopped — so this scenario fails against the
    # implementation it was written for.
    _init("init-with-a-front-door-name-whose-characters-have-no-glyph",
          python=[*INIT, "--front-door", "bad\u00a0name\u200b\u0890"]),
    _init("init-twice-changes-nothing", plane="minimal"),
    _init("init-on-a-plane-in-use-changes-nothing", plane="daily"),
    _init("init-appends-only-what-a-gitignore-of-its-own-is-missing",
          setup=_in_a_directory_with_its_own_gitignore),
    _init("init-adds-to-a-settings-file-in-its-own-layout",
          setup=_with_a_settings_file_of_its_own),
    _init("init-adds-to-a-compact-settings-file", setup=_with_compact_settings),
    _init("init-leaves-a-settings-file-claude-code-cannot-read",
          setup=_with_settings_claude_code_cannot_read,
          refusal="left it completely untouched"),
    _init("init-turns-an-opencode-allow-for-handoff-into-ask",
          setup=_with_an_opencode_json_that_allows_handoff),
    _init("init-names-a-file-where-a-baseline-directory-goes-and-leaves-it",
          setup=_with_a_file_where_inventory_goes, refusal="inventory/ can't be created"),
    _init("init-scaffolds-no-front-door-over-a-roster", setup=_with_a_persona_of_its_own),
    _init("init-on-a-plane-from-a-newer-charter-writes-nothing",
          setup=_from_a_newer_charter,
          refusal="declares schema 2, but this charter understands 1"),
    # ADR 0035 / spec decision 27, and the pair is the whole declaration: the first says the
    # DEFAULT diverges and exactly how, the second says nothing else did. Delete either and
    # the other stops meaning what it says.
    _init("init-inside-a-repository-does-not-colonise-it", setup=_in_a_git_repository,
          diverges=INIT_IN_A_REPO_DIVERGES),
    _init("init-inside-a-repository-when-asked-is-the-python-charters-init-byte-for-byte",
          setup=_in_a_git_repository,
          rust=[*INIT, "--plane-is-this-repo"]),
    _init("reinit-on-a-current-plane", python=["reinit"], plane="minimal"),
    _init("reinit-on-a-plane-in-use", python=["reinit"], plane="daily"),
    _init("reinit-heals-a-missing-baseline-directory", python=["reinit"], plane="minimal",
          setup=_without_inventory),
    _init("reinit-backfills-the-profiles-ignore-line", python=["reinit"], plane="minimal",
          setup=_without_the_profiles_ignore_line),
    _init("reinit-outside-a-plane-scaffolds-nothing", python=["reinit"],
          refusal="no control plane found"),
    _init("reinit-on-a-plane-from-a-newer-charter-writes-nothing", python=["reinit"],
          plane="minimal", setup=_from_a_newer_charter,
          refusal="declares schema 2, but this charter understands 1"),
]


# --------------------------------------------------------------------------------------------
# The repo commands, against a forge that is not one
# --------------------------------------------------------------------------------------------
#
# CI cannot reach a real forge, and a recorded HTTP exchange would test a transport neither
# implementation owns. So each side gets its own stand-in, beside its plane copy:
#
# - **A directory of bare repositories as the forge's git side.** Charter builds the HTTPS URL
#   itself (`https://github.com/acme/widget.git`) and never sees anything else; the side's own
#   `$HOME/.gitconfig` rewrites that prefix to the directory with `url.<base>.insteadOf`. So
#   what is compared is the real path — the URL built, the destination gated, `git clone` run
#   — and git's own config decides where the bytes come from, as it would behind a mirror.
# - **A recorded `gh` as the forge's API side.** A shell script first on `PATH` that answers
#   the calls `discover` makes with fixed JSON, and anything else with an error. Both sides
#   find it the same way — Python through `PATH`, Rust through `PATH` first (`forge.rs` says
#   why) — and it writes nothing, so it cannot trip the outside-the-plane check.
#
# What the stand-in cannot reach, and the Rust tests do instead (`repo_commands.rs`): a clone
# over a real network, and the credential helper git would call there.

#: The instant every commit in a stand-in forge is made at, so both sides' shas agree.
FORGE_ENV = {
    "GIT_AUTHOR_NAME": "Fixture User",
    "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
    "GIT_COMMITTER_NAME": "Fixture User",
    "GIT_COMMITTER_EMAIL": "fixture@example.invalid",
    "GIT_AUTHOR_DATE": "2026-05-04T10:00:00+00:00",
    "GIT_COMMITTER_DATE": "2026-05-04T10:00:00+00:00",
    "GIT_CONFIG_NOSYSTEM": "1",
    "PATH": "/usr/bin:/bin",
}


def _git(side: Path, *args: str, cwd: Path | None = None) -> str:
    """git, for a scenario's own setup, under the side's home — never either charter's."""
    done = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, text=True,
        env={**FORGE_ENV, "HOME": str(side / "home")},
    )
    if done.returncode != 0:
        raise SystemExit(f"setup: git {args} failed: {done.stderr}")
    return done.stdout.strip()


def _forge_repo(root: Path, name: str, branch: str) -> Path:
    """`acme/<name>` on the side's stand-in forge, one commit on `branch`; its source tree."""
    side = root.parent
    (side / "home").mkdir(parents=True, exist_ok=True)
    (side / "home" / ".gitconfig").write_text(
        f'[url "file://{side}/forge/acme/"]\n\tinsteadOf = https://github.com/acme/\n'
    )
    src = side / "forge" / f"{name}-src"
    src.mkdir(parents=True)
    _git(side, "init", "-q", "-b", branch, ".", cwd=src)
    (src / "README.md").write_text(f"# {name}\n")
    _git(side, "add", "-A", cwd=src)
    _git(side, "commit", "-q", "-m", "one", cwd=src)
    _git(side, "clone", "-q", "--bare", str(src), str(side / "forge" / "acme" / f"{name}.git"))
    return src


def _advance(root: Path, name: str, file: str, text: str) -> None:
    """One more commit on the stand-in forge's `acme/<name>`."""
    side = root.parent
    src = side / "forge" / f"{name}-src"
    (src / file).write_text(text)
    _git(side, "add", "-A", cwd=src)
    _git(side, "commit", "-q", "-m", f"add {file}", cwd=src)
    branch = _git(side, "symbolic-ref", "--short", "HEAD", cwd=src)
    _git(side, "push", "-q", str(side / "forge" / "acme" / f"{name}.git"), branch, cwd=src)


def _record(name: str, **over) -> dict:
    """An inventory record in the shape `discover` writes."""
    return {
        "name": name, "path_with_namespace": f"acme/{name}",
        "ssh_url": f"git@github.com:acme/{name}.git", "default_branch": "trunk",
        "kind": "app", "stack": "unknown", "description": "", "topics": [],
        "web_url": f"https://github.com/acme/{name}", "forge": "github", **over,
    }


def _inventory(root: Path, *records: dict) -> None:
    (root / "inventory").mkdir(parents=True, exist_ok=True)
    (root / "inventory" / "repos.json").write_text(json.dumps(
        {"group": "acme", "count": len(records), "repos": list(records)}, indent=2) + "\n")


def _a_widget_on_the_forge(root: Path) -> None:
    _forge_repo(root, "widget", "trunk")
    _inventory(root, _record("widget"))


def _a_widget_already_cloned(root: Path) -> None:
    """The same, and an operator's own clone of it already in the workspace."""
    _a_widget_on_the_forge(root)
    _git(root.parent, "clone", "-q", "https://github.com/acme/widget.git",
         str(root / "workspaces" / "alpha" / "widget"))


def _a_record_named_like_a_path(root: Path) -> None:
    _forge_repo(root, "widget", "trunk")
    _inventory(root, _record("../escape", web_url="https://github.com/acme/widget"))


def _a_record_whose_url_is_a_command(root: Path) -> None:
    # #335: `ext::` is a transport that runs a command, and the record is a tracked file.
    _inventory(root, _record("widget", web_url="",
                             ssh_url="ext::sh -c 'touch /tmp/charter-differential-pwned'"))


def _a_record_the_forge_does_not_have(root: Path) -> None:
    _forge_repo(root, "widget", "trunk")
    _inventory(root, _record("gone"))


def _behind_the_forge(root: Path) -> None:
    """A clean clone one commit behind its remote."""
    _a_widget_already_cloned(root)
    _advance(root, "widget", "NEWS.md", "news\n")


def _behind_with_work_in_the_tree(root: Path) -> None:
    _behind_the_forge(root)
    (root / "workspaces" / "alpha" / "widget" / "README.md").write_text("mine, uncommitted\n")


def _diverged_from_the_forge(root: Path) -> None:
    _behind_the_forge(root)
    clone = root / "workspaces" / "alpha" / "widget"
    (clone / "MINE.md").write_text("mine\n")
    _git(root.parent, "add", "-A", cwd=clone)
    _git(root.parent, "commit", "-q", "-m", "mine", cwd=clone)


def _clone_facts(name: str) -> "Callable[[Path], str]":
    """What a clone's `.git` says that the tree comparison cannot read."""

    def facts(root: Path) -> str:
        clone = root / "workspaces" / "alpha" / name
        if not (clone / ".git").is_dir():
            return "no clone"

        def ask(*args: str) -> str:
            return subprocess.run(
                ["git", "-C", str(clone), *args], capture_output=True, text=True,
                env={"PATH": "/usr/bin:/bin", "HOME": "/nonexistent", "GIT_CONFIG_NOSYSTEM": "1"},
            ).stdout

        exclude = clone / ".git" / "info" / "exclude"
        return (f"head: {ask('symbolic-ref', '-q', 'HEAD')}"
                f"commit: {ask('rev-parse', 'HEAD')}"
                f"config:\n{ask('config', '--local', '--list')}"
                f"exclude:\n{exclude.read_text() if exclude.exists() else '<none>'}\n"
                f"status:\n{ask('status', '--porcelain')}")

    return facts


def _stub_gh(root: Path, authed: bool = True) -> None:
    """A recorded `gh` first on the side's `PATH`: three repos, two trees, one failing probe."""
    bin_dir = root.parent / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    repos = [
        {"id": 11, "name": "widget", "full_name": "acme/widget", "default_branch": "trunk",
         "description": "The widget — made well", "html_url": "https://github.com/acme/widget",
         "ssh_url": "git@github.com:acme/widget.git", "topics": ["core", "ü"]},
        {"id": 12, "name": "api-gateway", "full_name": "acme/api-gateway",
         "default_branch": "main", "description": None,
         "html_url": "https://github.com/acme/api-gateway",
         "ssh_url": "git@github.com:acme/api-gateway.git", "topics": []},
        {"id": 13, "name": "legacy", "full_name": "acme/legacy", "default_branch": None,
         "description": "  x|y\nz ", "html_url": "https://github.com/acme/legacy",
         "ssh_url": "git@github.com:acme/legacy.git", "topics": []},
        # A name no directory can hold: dropped from the inventory by both.
        {"id": 14, "name": "../evil", "full_name": "acme/evil", "default_branch": "main",
         "description": "", "html_url": "https://github.com/acme/evil",
         "ssh_url": "git@github.com:acme/evil.git", "topics": []},
    ]
    (bin_dir / "repos.json").write_text(json.dumps(repos))
    (bin_dir / "tree-widget.json").write_text(json.dumps(
        {"tree": [{"path": "Cargo.toml"}, {"path": "src"}, {"path": "src/main.rs"}]}))
    (bin_dir / "tree-api.json").write_text(json.dumps({"tree": [{"path": "package.json"}]}))
    auth = "exit 0" if authed else 'echo "You are not logged into any GitHub hosts." >&2; exit 1'
    gh = bin_dir / "gh"
    gh.write_text(
        "#!/bin/sh\n"
        'case "$*" in\n'
        f'  "auth status --hostname github.com") {auth};;\n'
        f'  "api --hostname github.com orgs/acme/repos?per_page=100&page=1") '
        f"cat '{bin_dir}/repos.json'; exit 0;;\n"
        f'  "api --hostname github.com repos/acme/widget/git/trees/trunk") '
        f"cat '{bin_dir}/tree-widget.json'; exit 0;;\n"
        f'  "api --hostname github.com repos/acme/api-gateway/git/trees/main") '
        f"cat '{bin_dir}/tree-api.json'; exit 0;;\n"
        "esac\n"
        'echo "gh: Server Error (HTTP 502)" >&2\n'
        "exit 1\n"
    )
    gh.chmod(0o755)


def _stub_gh_logged_out(root: Path) -> None:
    _stub_gh(root, authed=False)


#: What Python charter writes beside a clone that the Rust one has no counterpart for: the
#: tmux frame's repaint counter and the in-flight record's directory. The app watches the
#: plane itself and draws its own progress, so neither is part of what `clone` leaves.
CLONE_FRAME_STATE = {
    ".charter/frame": "the tmux frame's repaint counter (`notify.plane_changed_everywhere`); "
    "the app has no frame to notify",
    ".charter/dispatch-inflight": "Python's in-flight record directory, created by the "
    "clone and emptied when it ends; the app draws its own progress",
}

#: A clone's `.git`, whose index and reflogs carry timestamps and inode numbers. `facts`
#: compares what it says instead.
def _clone_git(name: str) -> dict[str, str]:
    return {f"workspaces/alpha/{name}/.git": "compared through `facts`: the index and the "
            "reflogs carry timestamps and inodes no two runs share"}


# --------------------------------------------------------------------------------------- #
# gl-refresh and statusline                                                                 #
# --------------------------------------------------------------------------------------- #
#
# `gl-refresh` is the process that HOLDS THE FORGE CREDENTIAL, and the panels only read the
# file it leaves. So what these scenarios compare is that file: the same clone, the same
# recorded forge, the same entry.
#
# The cache is keyed by the clone's ABSOLUTE path, which is `python/plane/...` on one side and
# `rust/plane/...` on the other. That is not a difference in what either wrote — it is the two
# copies being in two places — so the file is compared through `facts`, with each side's own
# root taken out, rather than byte for byte by the tree walk.

ROLLUP_STATE = "SUCCESS"


def _a_clone_of_a_github_repo(root: Path) -> None:
    """A clone in `alpha` on `trunk` whose `origin` is a GitHub URL, and a recorded `gh`.

    No `insteadOf` here, unlike the clone scenarios: `git remote get-url` APPLIES the rewrite,
    so a plane set up that way answers `file:///…` and charter correctly decides it manages no
    such host — measured, and it is why this scenario builds the remote directly. `gl-refresh`
    runs no network git at all, so nothing has to be fetchable.
    """
    clone = root / "workspaces" / "alpha" / "widget"
    clone.mkdir(parents=True, exist_ok=True)
    _git(root.parent, "init", "-q", "-b", "trunk", ".", cwd=clone)
    _git(root.parent, "remote", "add", "origin", "https://github.com/acme/widget.git", cwd=clone)
    _stub_forge_gh(root)


def _stub_forge_gh(root: Path) -> None:
    """A recorded `gh` answering the two calls the status-line pair makes, and nothing else.

    The rollup comes back from `gh api graphql` whatever the query says, because the query is a
    multi-line GraphQL document and matching it in `sh` would test the shell rather than
    charter. What IS matched is the verb pair (`api graphql`), which is where the two
    implementations could differ in argument order — and they do not.
    """
    bin_dir = root.parent / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    (bin_dir / "pulls.json").write_text(json.dumps([{"number": 41}]))
    (bin_dir / "rollup.json").write_text(json.dumps(
        {"data": {"repository": {"ref": {"target": {
            "statusCheckRollup": {"state": ROLLUP_STATE}}}}}}))
    gh = bin_dir / "gh"
    gh.write_text(
        "#!/bin/sh\n"
        'case "$1 $2" in\n'
        f"  \"api graphql\") cat '{bin_dir}/rollup.json'; exit 0;;\n"
        "esac\n"
        'case "$*" in\n'
        f"  *\"pulls?state=open&head=acme:trunk&per_page=1\"*) cat '{bin_dir}/pulls.json'; "
        "exit 0;;\n"
        "esac\n"
        'echo "gh: Server Error (HTTP 502)" >&2\n'
        "exit 1\n"
    )
    gh.chmod(0o755)


def _glstate_facts(root: Path) -> str:
    """The forge cache, with each side's own plane root replaced by one word.

    Everything else about the file is compared exactly — the key's shape below the root, the
    field ORDER (`json.dumps` writes a dict in insertion order, so a writer that sorted would
    differ here), the timestamp, and every value.

    **Both spellings of the root are masked, and that is a real difference being declared
    rather than a path being tidied.** charter RESOLVES `$CHARTER_ROOT` (`root.find_root`:
    `p.resolve()`) and the Rust binary takes it as given (`plane::resolve`), so on macOS —
    where a scratch plane sits under `/var/folders/…`, itself a link to `/private/var/…` —
    the two write the same checkout under two spellings. That costs nothing on the read side
    by construction: the key is a path string and `cistate` falls back to `contain::resolved`
    for exactly this case, which `the_refresher_writes_the_key_the_panel_reads.rs` drives end
    to end. What it would cost is this comparison, which is about the ENTRY.
    """
    cache = root / ".charter" / "cache" / "glstate.json"
    if not cache.exists():
        return "no cache"
    text = cache.read_text()
    for spelling in {str(root), str(root.resolve())}:
        text = text.replace(spelling, "<plane>")
    return text


#: Python's in-flight record directory, created by `gl-refresh` and emptied when it ends. The
#: app draws its own progress from the plane, so the Rust binary keeps no such record.
REFRESH_INFLIGHT = {
    ".charter/dispatch-inflight": "Python's in-flight record directory, created by the "
    "refresh and emptied when it ends; the app watches the plane and draws its own progress",
}

#: The boundary both stdouts are cut at: charter's own **zone rule**, which `_boxed` draws as
#: `├───┤` under the workspace line.
#:
#: Above it is the whole of what M2.18 ports — the frame's top border and the identity row —
#: and it is compared byte for byte: the box's width, the workspace the nine-rung ladder
#: chose, its pin, its reinit tip, its open todos, its pieces, the count of other workspaces,
#: the separators between them, the truncation when the pane is too narrow and the padding out
#: to the right border.
#:
#: Not a marker invented for the comparison. It is the divider charter itself splices in
#: (`statusline._zone_rules`), which is why cutting here is a statement about the render rather
#: than about the test: everything in zone 1 is above it by construction.
IDENTITY_ROW = "\x1b[2m├"

#: What is below the cut, and why the two sides differ there.
BELOW_THE_RULE = (
    "below its zone rule charter draws the rest of the plane — a `git status` per clone, the "
    "worktree rows, the persona chips with vault health and memory counts, the alert list, the "
    "session strip and the right-aligned brand — and this build draws a line naming them as "
    "not drawn instead. Zone 1 and the frame ARE compared, byte for byte, above the rule."
)

#: One turn's payload, in the shape Claude Code hands the `statusLine` command.
A_TURN = json.dumps({
    "session_id": "s-1",
    "cwd": "/nowhere",
    "context_window": {
        "used_percentage": 42,
        "current_usage": {
            "cache_read_input_tokens": 90,
            "cache_creation_input_tokens": 10,
        },
    },
})


#: The same turn, from the session the fixture plane has a pointer for.
#:
#: The difference is the whole workspace ladder in one line: `A_TURN`'s `s-1` is an id no
#: pointer names, so it SHADOWS `$CHARTER_SESSION_ID` (`session.current(explicit)` takes the
#: payload first) and the ladder falls all the way to the built-in `default` — which is what a
#: real Claude Code turn looks like on a plane nobody has chosen a workspace in. This one lands
#: on `alpha`, where the fixture keeps the todos, the pieces and the structure marker.
A_TURN_IN_ALPHA = json.dumps({
    "session_id": SESSION,
    "cwd": "/nowhere",
    "context_window": {
        "used_percentage": 42,
        "current_usage": {
            "cache_read_input_tokens": 90,
            "cache_creation_input_tokens": 10,
        },
    },
})


def _pane(cols: int) -> dict[str, str]:
    """A pane width, pinned per scenario.

    Unpinned, both sides fall back to 80 because stdout is a pipe — the same answer twice, and
    therefore no test of the width at all. Pinning it is what lets one scenario render wide and
    another render into a pane too narrow for its own row.
    """
    return {"COLUMNS": str(cols)}


def _stale_the_workspaces_structure(root: Path) -> None:
    """Stamp `alpha` with an older layout version, which is what puts the reinit tip on the
    row — the one item there that reports something BROKEN, and the one that must therefore
    survive truncation ahead of every count beside it."""
    (root / "workspaces" / "alpha" / ".charter-structure").write_text("4\n")


#: A workspace directory named in a script charter does not validate the name of. Ten CJK
#: characters: twenty terminal COLUMNS and ten `len`. The row is rendered into a pane too
#: narrow for it on purpose, so the cut lands inside the name — which is the one place a port
#: that counted characters instead of columns cannot agree with charter.
CJK_WORKSPACE = "日本語の作業スペース"


def _a_workspace_whose_name_is_not_ascii(root: Path) -> None:
    ws = root / "workspaces" / CJK_WORKSPACE
    (ws / "memory").mkdir(parents=True, exist_ok=True)
    (ws / "refs").mkdir(parents=True, exist_ok=True)
    (ws / "todos").mkdir(parents=True, exist_ok=True)
    (ws / "workspace.md").write_text("# ws\n")
    (ws / "workspace.json").write_text("{}\n")
    (ws / "memory" / "MEMORY.md").write_text("# Memory\n")
    (ws / "refs" / "README.md").write_text("# Refs\n")
    # Current, so the row carries the name and the counts and no repair tip.
    (ws / ".charter-structure").write_text("5\n")


def _pieces_that_have_and_have_not_reported(root: Path) -> None:
    """Four worktrees under `alpha`: one done, one abandoned, and two that have said nothing
    since they were claimed — three days ago and two hours ago.

    The cell reports the OLDEST silence, and `3d` against `2h` is exactly the pair that does
    not sort lexically. Both sides measure it from the same instant: charter's clock is pinned
    by `time_machine` and the Rust binary is handed `--now`.
    """
    base = root / "workspaces" / "alpha" / ".worktrees" / "svc"
    for piece in ("shipped", "dropped", "quiet-for-days", "quiet-for-hours"):
        (base / piece).mkdir(parents=True, exist_ok=True)

    def when(hours: float) -> str:
        return (NOW - timedelta(hours=hours)).isoformat(timespec="seconds")

    lines = [
        {"ts": when(10), "event": "claimed", "repo": "svc", "piece": "shipped"},
        {"ts": when(9), "event": "done", "repo": "svc", "piece": "shipped"},
        {"ts": when(10), "event": "claimed", "repo": "svc", "piece": "dropped"},
        {"ts": when(8), "event": "abandoned", "repo": "svc", "piece": "dropped",
         "reason": "the branch was already merged"},
        {"ts": when(72), "event": "claimed", "repo": "svc", "piece": "quiet-for-days"},
        {"ts": when(2), "event": "claimed", "repo": "svc", "piece": "quiet-for-hours"},
        # A line no parser can read: it is skipped and the rest of the log still counts. An
        # append-only log collects these from workers that were killed mid-write.
        None,
    ]
    log = root / "workspaces" / "alpha" / "pieces" / f"{HOSTNAME}.jsonl"
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text("".join(
        "not json at all\n" if line is None else json.dumps(line, sort_keys=True) + "\n"
        for line in lines
    ))


STATUSLINE_SCENARIOS = [
    Scenario(
        # ADR 0019's own sentence, as a test: "a `cleanup` that removes it deletes the record
        # silently". Both charters must leave the same `.charter/sessions/<sid>.usage`, byte
        # for byte and mode for mode, or one of them has stopped writing the only copy of this
        # session's token history that exists anywhere. The ROW is compared too, now there is
        # one.
        name="statusline-records-the-turns-tokens",
        plane="daily",
        python=["statusline"],
        stdin=A_TURN,
        env=_pane(120),
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # Early in a session and right after `/compact` there is no usage at all. Recording a
        # zero there would invent a turn — and a `0/0` divided — so NEITHER side may leave a
        # file behind, which is what an identical (and unchanged) plane says here.
        name="statusline-with-no-numbers-records-nothing",
        plane="daily",
        python=["statusline"],
        stdin=json.dumps({"session_id": "s-1", "cwd": "/nowhere"}),
        env=_pane(120),
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # The outermost boundary of the whole subprocess: a payload that will not parse must
        # still leave a plane nobody has to repair — and must still draw the row, because the
        # row is read off the plane and not off the payload.
        name="statusline-survives-a-payload-that-is-not-json",
        plane="daily",
        python=["statusline"],
        stdin="not json {{{",
        env=_pane(120),
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # A plane with no `workspaces/` at all: the ladder ends on the built-in `default`, and
        # the row says so with `ws 0` rather than inventing a workspace or leaving the count
        # out. The frame is still drawn, because the frame is what makes the row a row.
        name="statusline-identity-row-on-a-plane-with-nothing-in-it",
        plane="minimal",
        python=["statusline"],
        stdin=A_TURN,
        env=_pane(120),
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # `$CHARTER_WORKSPACE` is the one rung that earns a pin: that session cannot be moved
        # with `charter ws use`, and the `*` beside the name is where a reader finds that out.
        # The workspace it names is NOT the one the session pointer holds, so a port that read
        # the wrong rung would draw the wrong name and no pin at once.
        name="statusline-identity-row-when-the-environment-pins-the-workspace",
        plane="daily",
        python=["statusline"],
        stdin=A_TURN,
        env={**_pane(120), "CHARTER_WORKSPACE": "beta"},
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # The active workspace's open todos, beside the name whose todos they are. `alpha` has
        # one; `MEMORY.md` in the same store is not a todo, and a store that answered "two"
        # would be counting the index.
        name="statusline-identity-row-counts-the-active-workspaces-todos",
        plane="daily",
        python=["statusline"],
        stdin=A_TURN_IN_ALPHA,
        env=_pane(120),
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # A stale layout puts the repair tip on the row, ahead of every count beside it.
        name="statusline-identity-row-flags-a-stale-workspace-before-its-counts",
        plane="daily",
        python=["statusline"],
        stdin=A_TURN_IN_ALPHA,
        env=_pane(120),
        setup=_stale_the_workspaces_structure,
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # …and the same plane in a pane too narrow to hold it, which is where the order of the
        # row becomes its truncation order.
        name="statusline-identity-row-gives-up-counts-before-the-repair-tip",
        plane="daily",
        python=["statusline"],
        stdin=A_TURN_IN_ALPHA,
        env=_pane(46),
        setup=_stale_the_workspaces_structure,
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # The piece cell: counts, and the oldest silence among the ones that have said nothing.
        name="statusline-identity-row-counts-the-workspaces-pieces",
        plane="daily",
        python=["statusline"],
        stdin=A_TURN_IN_ALPHA,
        env=_pane(120),
        setup=_pieces_that_have_and_have_not_reported,
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
    Scenario(
        # **The width test.** A workspace directory named in CJK, rendered into a pane too
        # narrow for it, so the cut lands inside the name. charter measures a column with
        # `unicodedata.east_asian_width`; a port that counted characters draws this row one
        # cell short per glyph and its right border stops lining up with every other row's.
        name="statusline-identity-row-measures-columns-and-not-characters",
        plane="daily",
        python=["statusline"],
        stdin=A_TURN,
        env={**_pane(30), "CHARTER_WORKSPACE": CJK_WORKSPACE},
        setup=_a_workspace_whose_name_is_not_ascii,
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
    ),
]

def _a_clone_of_an_unmanaged_host(root: Path) -> None:
    clone = root / "workspaces" / "alpha" / "widget"
    clone.mkdir(parents=True, exist_ok=True)
    _git(root.parent, "init", "-q", "-b", "trunk", ".", cwd=clone)
    _git(root.parent, "remote", "add", "origin", "git@git.example.invalid:acme/widget.git",
         cwd=clone)


GL_REFRESH_SCENARIOS = [
    Scenario(
        name="gl-refresh-in-a-workspace-with-no-clones",
        plane="daily",
        python=["gl-refresh", "-w", "alpha"],
    ),
    Scenario(
        # The one that matters: the entry a panel then reads. Same key below the root, same
        # branch, same instant, same change, same CI word, same sigil — and the same private
        # mode on the file, which the tree walk checks.
        name="gl-refresh-writes-what-the-forge-said",
        plane="daily",
        setup=_a_clone_of_a_github_repo,
        python=["gl-refresh", "-w", "alpha"],
        facts=_glstate_facts,
        ignore={
            **REFRESH_INFLIGHT,
            ".charter/cache/glstate.json": "the key is the clone's absolute path, which is "
            "each side's own copy; compared through `facts` with the root taken out",
            **_clone_git("widget"),
        },
    ),
    Scenario(
        # A repo charter manages no forge for. `resolve_host` answers None and NEITHER side
        # asks anything — the entry still exists, naming the branch and the instant, because
        # "the last refresh found nothing" and "nobody has looked" are different answers.
        name="gl-refresh-leaves-an-entry-for-a-host-it-does-not-manage",
        plane="daily",
        setup=_a_clone_of_an_unmanaged_host,
        python=["gl-refresh", "-w", "alpha"],
        facts=_glstate_facts,
        ignore={
            **REFRESH_INFLIGHT,
            ".charter/cache/glstate.json": "the key is the clone's absolute path; compared "
            "through `facts`",
            **_clone_git("widget"),
        },
    ),
]


REPO_SCENARIOS = [
    Scenario(
        name="clone-checks-out-the-default-branch-and-wires-the-layer",
        plane="daily",
        setup=_a_widget_on_the_forge,
        python=["clone", "widget", "-w", "alpha"],
        facts=_clone_facts("widget"),
        ignore={**CLONE_FRAME_STATE, **_clone_git("widget")},
    ),
    Scenario(
        name="clone-of-a-repo-already-cloned-records-and-wires-it",
        plane="daily",
        setup=_a_widget_already_cloned,
        python=["clone", "widget", "-w", "alpha"],
        facts=_clone_facts("widget"),
        ignore={**CLONE_FRAME_STATE, **_clone_git("widget")},
    ),
    Scenario(
        name="clone-refuses-a-record-named-like-a-path",
        plane="daily",
        setup=_a_record_named_like_a_path,
        python=["clone", "../escape", "-w", "alpha"],
        refusal="✗ '../escape': not cloned — '../escape' is not a name — it is a path.",
        ignore=CLONE_FRAME_STATE,
    ),
    Scenario(
        name="clone-refuses-a-url-it-did-not-build",
        plane="daily",
        setup=_a_record_whose_url_is_a_command,
        python=["clone", "widget", "-w", "alpha"],
        refusal="✗ 'widget': not cloned — its inventory record carries no HTTPS clone URL",
        ignore=CLONE_FRAME_STATE,
    ),
    Scenario(
        name="clone-of-a-repo-not-in-the-inventory",
        plane="daily",
        setup=_a_widget_on_the_forge,
        python=["clone", "nope", "-w", "alpha"],
        refusal="✗ No matching repos.",
        pins_the_clock=False,
    ),
    Scenario(
        name="clone-of-a-repo-the-forge-does-not-have",
        plane="daily",
        setup=_a_record_the_forge_does_not_have,
        python=["clone", "gone", "-w", "alpha"],
        refusal="✗ gone: clone failed — no access, network, or gh isn't authed "
        "(`gh auth status`). Skipping.",
        ignore=CLONE_FRAME_STATE,
    ),
    Scenario(
        name="sync-fast-forwards-a-clean-clone",
        plane="daily",
        setup=_behind_the_forge,
        python=["sync", "-w", "alpha"],
        pins_the_clock=False,
        facts=_clone_facts("widget"),
        ignore=_clone_git("widget"),
    ),
    Scenario(
        name="sync-skips-a-clone-with-uncommitted-work",
        plane="daily",
        setup=_behind_with_work_in_the_tree,
        python=["sync", "-w", "alpha"],
        pins_the_clock=False,
        facts=_clone_facts("widget"),
        ignore=_clone_git("widget"),
    ),
    Scenario(
        name="sync-leaves-a-diverged-clone-as-it-is",
        plane="daily",
        setup=_diverged_from_the_forge,
        python=["sync", "-w", "alpha"],
        pins_the_clock=False,
        facts=_clone_facts("widget"),
        ignore=_clone_git("widget"),
    ),
    Scenario(
        name="sync-all-workspaces-reaches-the-clone-in-one-of-them",
        plane="daily",
        setup=_behind_the_forge,
        python=["sync", "--all"],
        pins_the_clock=False,
        facts=_clone_facts("widget"),
        ignore=_clone_git("widget"),
    ),
    Scenario(
        name="sync-with-nothing-cloned",
        plane="daily",
        python=["sync", "-w", "beta"],
        pins_the_clock=False,
    ),
    Scenario(
        name="discover-writes-the-inventory-and-the-topology",
        plane="daily",
        setup=_stub_gh,
        python=["discover"],
        pins_the_clock=False,
    ),
    Scenario(
        name="discover-without-probing-or-docs",
        plane="daily",
        setup=_stub_gh,
        python=["discover", "--no-probe", "--no-docs"],
        pins_the_clock=False,
    ),
    Scenario(
        name="discover-refuses-when-the-forge-cli-is-logged-out",
        plane="daily",
        setup=_stub_gh_logged_out,
        python=["discover"],
        pins_the_clock=False,
        refusal="gh is not authenticated for github.com. Run: gh auth login",
    ),
]

# ---------------------------------------------------------------------------------------
# M2.11: `charter status` and `charter docs generate`.
#
# `status` is what an operator types when something has already gone wrong, so its whole
# output is the assertion — the header's counts, which rung chose the workspace, the table's
# column widths, and the three answers the NOTE column may give. It writes nothing, so the
# tree comparison proves only that; what these scenarios are for is stdout, byte for byte.
#
# NOT here, and deliberately: the nested-plane notice, which names two ABSOLUTE paths. Those
# are two different directories on the two sides of this comparison, so it is asserted in
# `crates/charter-cli/tests/status.rs`, where one plane is built and one binary runs.


def _clones_of_every_shape(root: Path) -> None:
    """Everything a workspace can hold, in `alpha`, so one table draws all of it:

    a clean clone on the branch it was cloned on; a clone with work in its tree, on a branch
    of its own, whose `stack` the inventory knows; a directory that is no repository; and a
    repo the manifest NAMES with nothing on disk. The last two are the pair charter#1043 and
    M2.3 are about — membership is not presence, and `status` draws presence.

    `widget`'s stack is `node-monorepo` on purpose: thirteen characters, which is what
    charter#592 measured pushing every monorepo row one column right of every other.

    Neither clone may be called `svc` or `tool`: `alpha` already holds directories of both
    names (and `tool` is a nested plane of its own), and `git clone` into one refuses.
    """
    _forge_repo(root, "widget", "trunk")
    _forge_repo(root, "gadget", "main")
    _inventory(root,
               _record("widget", stack="node-monorepo", kind="service"),
               _record("gadget", stack="rust", default_branch="main"),
               _record("absent", stack="go"),
               _record("never-cloned", stack="python"))
    alpha = root / "workspaces" / "alpha"
    side = root.parent
    _git(side, "clone", "-q", "https://github.com/acme/widget.git", str(alpha / "widget"))
    _git(side, "clone", "-q", "https://github.com/acme/gadget.git", str(alpha / "gadget"))
    _git(side, "checkout", "-q", "-b", "feature/x", cwd=alpha / "gadget")
    (alpha / "gadget" / "README.md").write_text("mine, uncommitted\n")
    (alpha / "plaindir").mkdir()
    (alpha / "plaindir" / "README.md").write_text("# not a repo\n")
    manifest = json.loads((alpha / "workspace.json").read_text())
    manifest["repos"] = [{"name": "widget", "branch": "trunk"},
                         {"name": "gadget", "branch": "main"},
                         {"name": "absent", "branch": "main"}]
    (alpha / "workspace.json").write_text(json.dumps(manifest, indent=2) + "\n")


#: `.git` of each clone the table draws: an index and reflogs carrying timestamps and inode
#: numbers. `status` writes nothing at all, so there is no `facts` to compare instead — what
#: these scenarios assert is entirely on stdout.
STATUS_CLONE_GIT = {
    f"workspaces/alpha/{name}/.git": "read by `status`, never written; the index and the "
    "reflogs carry timestamps and inodes no two runs share"
    for name in ("widget", "gadget")
}

#: An inventory with one repo in it, so `docs generate` has something to render.
DOCS_RECORD = {"name": "widget", "path_with_namespace": "acme/widget",
               "ssh_url": "git@github.com:acme/widget.git", "default_branch": "trunk",
               "kind": "app", "stack": "rust", "description": "The widget — made well",
               "topics": [], "web_url": "https://github.com/acme/widget", "forge": "github"}


def _a_readme_carrying_the_roster_block(root: Path) -> None:
    """A README with hand-written prose on both sides of the generated block.

    The prose is the assertion: charter owns the span between the markers and nothing else,
    and a port that rewrote the file whole would pass a test that only looked at the roster.
    """
    _inventory(root, DOCS_RECORD)
    (root / "README.md").write_text(
        "# The plane\n\nHand written, and it stays that way.\n\n"
        "<!-- BEGIN personas — GENERATED by `charter docs`; do not edit by hand. -->\n"
        "stale — from a charter two versions ago\n"
        "<!-- END personas -->\n\n"
        "## Mine\n\nAlso hand written.\n")


def _a_readme_carrying_no_block(root: Path) -> None:
    """A hand-written README with no markers: never appended to, never rewritten."""
    _inventory(root, DOCS_RECORD)
    (root / "README.md").write_text("# The plane\n\nNo roster here.\n")


def _an_inventory_to_render(root: Path) -> None:
    _inventory(root, DOCS_RECORD)


def _a_persona_whose_frontmatter_holds_lines_that_are_not_pairs(root: Path) -> None:
    """`devops`, rewritten so every roster field sits BELOW a line with no colon in it.

    The roster's capability column is read from `tools:` — through `personas::frontmatter`,
    which is the one reader `Persona::role` now shares (charter-app #67). The bug that issue
    reports is a reader that STOPS at a frontmatter line without a colon where charter skips
    it, so this plants the three shapes that produce one — a blank line, a line charter's
    format has no meaning for, and a value wrapped onto a continuation line — above the
    field the README is about to print. charter reads `Bash, Read`; a reader that gave up on
    any of those three lines prints the em dash for a persona that declared its tools.

    `tools:` is also written TWICE, because `persona.parse` is `dict(pairs)` and the second
    line is the one charter keeps. A reader that answered with the first would print
    `Bash, Read` from the wrong line and look right for the wrong reason, so the two values
    are different and only the lower one is correct.

    **The README with the markers is planted on purpose, and the scenario is worth nothing
    without it.** The `daily` fixture has no `README.md`; `roster::splice` returns `None`
    when either marker is missing, so `docs generate` writes no roster block and the
    persona's `tools:` never reaches a file either side compares. Without this the scenario
    reported `ok` under a mutation that reintroduced #67's bug in the shared reader —
    measured on CI 35526015591, which is why it is here.
    """
    _inventory(root, DOCS_RECORD)
    (root / "README.md").write_text(
        "# The plane\n\n"
        "<!-- BEGIN personas — GENERATED by `charter docs`; do not edit by hand. -->\n"
        "<!-- END personas -->\n")
    (root / "personas" / "devops" / "persona.md").write_text(
        "---\n"
        "\n"
        "name: devops\n"
        "this line has no colon and charter walks straight past it\n"
        "vault: devops\n"
        "delegate-when: CI/CD pipelines, k8s deploys\n"
        "    a continuation line, which is how a long value gets wrapped\n"
        "tools: Write, Edit\n"
        "tools: Bash, Read\n"
        "role: DevOps Engineer\n"
        "---\n"
        "\n# DevOps Engineer\n\nThe body, which is nobody's frontmatter.\n"
    )


STATUS_SCENARIOS = [
    Scenario(
        name="status-on-a-plane-whose-workspaces-hold-nothing",
        plane="daily",
        python=["status"],
        pins_the_clock=False,
    ),
    Scenario(
        name="status-detailing-every-workspace",
        plane="daily",
        python=["status", "--all"],
        pins_the_clock=False,
    ),
    Scenario(
        name="status-of-a-workspace-named-on-the-command-line",
        plane="daily",
        python=["status", "-w", "beta"],
        pins_the_clock=False,
    ),
    Scenario(
        name="status-from-inside-a-workspaces-own-tree",
        plane="daily",
        # The cwd rung, which is the one that cannot be planted as a file — and the header
        # has to name it, or it explains the answer by naming a rung that did not decide it.
        cwd="workspaces/alpha/svc",
        python=["status"],
        pins_the_clock=False,
    ),
    Scenario(
        name="status-on-a-plane-with-no-workspaces-at-all",
        plane="minimal",
        # No pane id in this harness, so the last rung says WHY nothing answered rather than
        # only that nothing did.
        python=["status"],
        pins_the_clock=False,
    ),
    Scenario(
        name="status-draws-a-clean-clone-a-dirty-one-and-neither-of-the-other-two",
        plane="daily",
        setup=_clones_of_every_shape,
        python=["status"],
        pins_the_clock=False,
        ignore=STATUS_CLONE_GIT,
    ),
    Scenario(
        name="docs-generate-refuses-an-empty-inventory",
        plane="daily",
        python=["docs", "generate"],
        pins_the_clock=False,
        same_stderr=True,
        refusal="Inventory is empty — run `charter discover` first.",
    ),
    Scenario(
        name="bare-docs-still-generates",
        plane="daily",
        setup=_an_inventory_to_render,
        python=["docs"],
        pins_the_clock=False,
    ),
    Scenario(
        name="docs-generate-rewrites-only-the-readmes-roster-block",
        plane="daily",
        setup=_a_readme_carrying_the_roster_block,
        python=["docs", "generate"],
        pins_the_clock=False,
    ),
    Scenario(
        name="docs-generate-leaves-a-readme-with-no-block-exactly-as-it-is",
        plane="daily",
        setup=_a_readme_carrying_no_block,
        python=["docs", "generate"],
        pins_the_clock=False,
    ),
    Scenario(
        name="docs-generate-reads-a-roster-field-below-a-frontmatter-line-that-is-not-a-pair",
        plane="daily",
        setup=_a_persona_whose_frontmatter_holds_lines_that_are_not_pairs,
        python=["docs", "generate"],
        pins_the_clock=False,
    ),
]

# ---------------------------------------------------------------------------------------
# M2.2: the memory commands. `charter recall`'s output is read by agents at session start,
# so these compare stdout AND stderr byte for byte, and the setups below plant the states a
# fixture cannot carry — a link out of the plane, a FIFO, an unreadable store.

#: The flags a Rust `recall` needs that Python's does not: this binary resolves neither the
#: active workspace nor the active persona, so both are named. Python is given the same
#: flags, so the two search the same bases.
RECALL_OWNERS = ["--persona", "devops", "-w", "alpha"]


def _ephemeral_scratch(root: Path) -> None:
    """One ephemeral memory for `devops` in this session's bucket."""
    d = root / ".charter" / "persona-state" / "ephemeral" / SESSION / "devops"
    d.mkdir(parents=True, exist_ok=True)
    (d / "scratch-one.md").write_text(
        "# Scratch one\n\n_2026-05-01 10:00 · ephemeral_\n\nhello\n", encoding="utf-8"
    )


def _entries_the_gate_refuses(root: Path) -> None:
    """Every kind of entry `memstore.files` refuses, beside the journal's real memories.

    A link out of the plane whose heading and body match the query — the shape that made
    `duplicate_of` leak a heading and a similarity oracle in M1.1 — a FIFO, which would
    block a reader for ever, a file past the 1 MiB bound, and a directory named like a
    memory. Plus a link that lands INSIDE the plane, which is a memory, and which a search
    labels by where it lands.
    """
    m = root / "workspaces" / "alpha" / "memory"
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (outside / "secret.md").write_text(
        "# Board minutes Mondays\n\nCONFIDENTIAL Mondays\n", encoding="utf-8"
    )
    (m / "leak.md").symlink_to(outside / "secret.md")
    os.mkfifo(m / "fifo.md")
    (m / "big.md").write_text("# Big Mondays\n" + "x" * 1_048_577, encoding="utf-8")
    (m / "dir.md").mkdir()
    (m / "inside.md").symlink_to(
        root / "personas" / "_shared" / "memory" / "the-plane-is-the-unit-of-work.md"
    )


def _nested_refs(root: Path) -> None:
    """Refs that nest, one subdirectory linked inside the plane and one linked out of it."""
    r = root / "personas" / "devops" / "refs"
    (r / "sub" / "deep").mkdir(parents=True, exist_ok=True)
    (r / "sub" / "runbook.md").write_text("# Runbook\nrestart the thing\n", encoding="utf-8")
    (r / "sub" / "deep" / "x.md").write_text("no heading here\n", encoding="utf-8")
    (r / "linked").symlink_to(root / "workspaces" / "alpha" / "memory")
    out = root.parent / "out-refs"
    out.mkdir(parents=True, exist_ok=True)
    (out / "s.md").write_text("# Outside ref\nleaked\n", encoding="utf-8")
    (r / "escape").symlink_to(out)


def _dates_every_way(root: Path) -> None:
    """A memory dated by each rule `memory_date` has, and one it cannot date.

    CRLF line ends (text mode reads them as `\\n`, which decides where `^` matches), a stamp
    on a later line, a date only in the filename, a stamp that does not parse — which falls
    to the filename rather than to a later stamp — and no date at all.
    """
    m = root / "workspaces" / "alpha" / "memory"
    (m / "crlf.md").write_bytes(
        b"# CRLF title\r\n\r\n_2026-04-01 10:00 \xc2\xb7 persistent_\r\n\r\nbody line mondays\r\n"
    )
    (m / "nohead.md").write_text("plain text mondays mondays\n", encoding="utf-8")
    (m / "20260401-x.md").write_text("# Dated by name\nmondays\n", encoding="utf-8")
    (m / "late.md").write_text("# Late stamp\ntext\n_2026-04-02 x_\nmondays\n", encoding="utf-8")
    (m / "bad.md").write_text("# Bad stamp\n_2026-13-02 x_\n", encoding="utf-8")


def _journal_unreadable(root: Path) -> None:
    """`alpha`'s journal at mode 000: a store charter cannot list is not an empty one."""
    os.chmod(root / "workspaces" / "alpha" / "memory", 0)


def _a_fifo_in_the_journal(root: Path) -> None:
    os.mkfifo(root / "workspaces" / "alpha" / "memory" / "pipe.md")


def _a_journal_entry_linked_out(root: Path) -> None:
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (outside / "secret.md").write_text("# Secret\n\nSECRET\n", encoding="utf-8")
    (root / "workspaces" / "alpha" / "memory" / "leak.md").symlink_to(outside / "secret.md")


def _duplicates_to_curate(root: Path) -> None:
    """Two exact duplicates of a journal entry (one retitled), a near duplicate, and a
    rule-shaped memory — none of them indexed."""
    m = root / "workspaces" / "alpha" / "memory"
    t = ("# The API returns 418 on Mondays\n\n_2026-03-05 10:00 · persistent_\n\n"
         "The API returns 418 on Mondays\n")
    (m / "20260305-100000-the-api-returns-418-on-mondays.md").write_text(t, encoding="utf-8")
    (m / "20260306-100000-dup-again.md").write_text(
        t.replace("# The API", "# Another title"), encoding="utf-8")
    (m / "20260307-100000-near.md").write_text(
        "# Near\nThe API returns 418 on Tuesdays and Mondays\n", encoding="utf-8")
    (m / "20260308-100000-rule.md").write_text(
        "# Standing rule: never deploy on Fridays\nThis is a standing rule. You must not "
        "deploy; never ever. Always check. The rule is simple.\n", encoding="utf-8")


def _archive_names_taken(root: Path) -> None:
    """Two copies of an entry, and the name the first would be archived under already taken."""
    m = root / "workspaces" / "alpha" / "memory"
    (m / "archive").mkdir(exist_ok=True)
    t = (m / "20260302-091200-the-api-returns-418-on-mondays.md").read_text(encoding="utf-8")
    (m / "20260309-000000-copy.md").write_text(t, encoding="utf-8")
    (m / "20260310-000000-copy2.md").write_text(t, encoding="utf-8")
    (m / "archive" / "20260309-000000-copy.md").write_text("taken\n", encoding="utf-8")
    (m / "archive" / "20260309-000000-copy-2.md").write_text("taken too\n", encoding="utf-8")


def _journal_index_linked_out(root: Path) -> None:
    """`alpha`'s MEMORY.md a link to an operator's file, and an unindexed entry to repair."""
    m = root / "workspaces" / "alpha" / "memory"
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (outside / "idx").write_text("PRECIOUS\n", encoding="utf-8")
    (m / "MEMORY.md").unlink()
    (m / "MEMORY.md").symlink_to(outside / "idx")


def _journal_archive_linked_out(root: Path) -> None:
    """`archive/` a link out of the plane, and an exact duplicate that would be moved there."""
    m = root / "workspaces" / "alpha" / "memory"
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (m / "archive").symlink_to(outside)
    t = (m / "20260302-091200-the-api-returns-418-on-mondays.md").read_text(encoding="utf-8")
    (m / "20260309-000000-copy.md").write_text(t, encoding="utf-8")


def _persona_index_linked_out(root: Path) -> None:
    m = root / "personas" / "devops" / "memory"
    outside = root.parent / "outside"
    outside.mkdir(parents=True, exist_ok=True)
    (outside / "idx").write_text("PRECIOUS\n", encoding="utf-8")
    (m / "MEMORY.md").unlink()
    (m / "MEMORY.md").symlink_to(outside / "idx")


#: A refusal naming two absolute paths, which differ between the two plane copies.
ABSOLUTE_PATHS = (r"'/[^']*'", "each side's plane copy lives at its own absolute path")
# --------------------------------------------------------------------------------------------
# `charter save` and `charter git-policy`, against a bare repository beside the plane
# --------------------------------------------------------------------------------------------
#
# A push is the one thing in this suite that is MEANT to write outside the plane copy, and it
# must never reach a forge. So each side gets a bare repository beside its own plane and pushes
# to that, through the same `url.<base>.insteadOf` rewrite the repo scenarios use.
#
# **`origin` is the SSH form and the rewrite is keyed on the HTTPS one**, and that ordering is
# what makes the stand-in work at all. `git remote get-url` APPLIES `insteadOf`: keyed on the
# URL charter reads, it would hand both implementations a `file://` origin, both would answer
# "that is on no forge charter knows", and the scenario would pass while pushing nothing.
# Keyed on the HTTPS prefix, `get-url` returns the SSH URL untouched, charter rewrites it to
# HTTPS itself — the rule `docs/git-policy.md` is about — and git maps that onto the bare
# repository. Measured both ways.
#
# What the two sides cannot agree on is the commit's SHA. The Rust git runner clears the
# environment, so the `GIT_*_DATE` this harness pins for Python does not reach it and the two
# commits carry different committer dates. `_plane_facts` therefore compares what each commit
# CONTAINS — its tree, its subject, its file list — and the sha in the message is masked.

#: The plane's own remote, as a URL charter has to rewrite before it can use it.
PLANE_ORIGIN = "git@github.com:acme/plane.git"


def _identity(side: Path, extra: "list[str]" = ()) -> None:
    """A git identity for the side, and whatever else its scenario needs in `$HOME`."""
    home = side / "home"
    home.mkdir(parents=True, exist_ok=True)
    lines = ["[user]", "\tname = Fixture User", "\temail = fixture@example.invalid", *extra]
    (home / ".gitconfig").write_text("\n".join(lines) + "\n")


def _plane_repo(root: Path, *, origin: "str | None" = PLANE_ORIGIN, rewrite: bool = True,
                extra_config: "list[str]" = ()) -> Path:
    """The fixture plane as a git repository with one commit, and a bare remote beside it."""
    side = root.parent
    rule = [f'[url "file://{side}/forge/acme/"]', "\tinsteadOf = https://github.com/acme/"]
    # `extra_config` goes in AFTER the setup commit: a `commit.gpgsign = true` written before it
    # signs the fixture's own first commit, and there is no signer.
    _identity(side, rule if rewrite else [])
    bare = side / "forge" / "acme" / "plane.git"
    bare.mkdir(parents=True)
    _git(side, "init", "-q", "--bare", "-b", "main", ".", cwd=bare)
    _git(side, "init", "-q", "-b", "main", ".", cwd=root)
    # What `charter init` writes: the plane's machine-local state is not committed.
    (root / ".gitignore").write_text(".charter/\n")
    _git(side, "add", "-A", cwd=root)
    _git(side, "commit", "-q", "-m", "the plane", cwd=root)
    _git(side, "push", "-q", str(bare), "HEAD:refs/heads/main", cwd=root)
    if origin:
        _git(side, "remote", "add", "origin", origin, cwd=root)
    if extra_config:
        _identity(side, [*(rule if rewrite else []), *extra_config])
    return bare


#: A remote that is a path on this machine rather than a URL on a forge. `file://` is what the
#: `insteadOf` rewrite produces; a bare path is what someone writes by hand.
LOCAL_REMOTE = re.compile(r"^(file://|/|\.{1,2}/)")


def _forge_trap(root: Path, side: str, scenario: Scenario) -> list[str]:
    """Refuse a scenario whose plane `origin` resolved to a local path without saying so.

    **The trap this closes, in full, because it is silent and it is easy to fall into.**

    A differential scenario cannot push to a real forge, so it stands a bare repository up
    beside the plane and rewrites the forge's URL onto it with `url.<local>.insteadOf`. But
    `git remote get-url` — which is how `planegit.origin_https` reads the plane's remote —
    **applies `insteadOf`**. So if the rewrite is keyed on the URL that is actually in
    `.git/config`, charter reads back `file:///…`, cannot place it on any forge it knows, and
    answers "there is no forge here" rather than pushing. Both implementations answer that
    identically. The scenario is GREEN, the diff is empty, and neither side ever entered the
    code the scenario was written for. Measured on this suite: keyed the other way — rewrite on
    the HTTPS base, `origin` left in the SSH form — `get-url` hands back the SSH URL, charter
    does the HTTPS rewrite itself, and git maps the result onto the bare repository.

    `env_clear()` in the Rust git runner does not protect against it: the rewrite arrives
    through CONFIG, not the environment, and `HOME` is passed to every git child on purpose
    (`worktree::git`'s module docs say why), so `~/.gitconfig` reaches every call charter makes.

    So the origin is measured here, with the side's own `$HOME`, exactly as charter will see
    it — before the command runs, because the command may write git config of its own. A local
    answer is a scenario that is probably not testing what it says; it passes only if it
    declares `local_origin_why`. And a `local_origin_why` on a scenario whose origin is NOT
    local fails too, so the note cannot outlive what it records.
    """
    if not (root / ".git").exists():
        return []
    done = subprocess.run(
        ["git", "-C", str(root), "remote", "get-url", "origin"],
        capture_output=True, text=True,
        env={**FORGE_ENV, "HOME": str(root.parent / "home")},
    )
    origin = done.stdout.strip() if done.returncode == 0 else ""
    local = bool(origin) and bool(LOCAL_REMOTE.match(origin))
    if local and not scenario.local_origin_why:
        return [
            f"    {side}'s plane origin resolves to {origin!r} — a path on this machine, not a "
            "forge charter can place. Every forge answer this scenario compares is 'there is "
            "no forge here', on both sides, so it proves nothing. Key url.insteadOf on the "
            "HTTPS base and leave origin in the SSH form (see _plane_repo), or set "
            "local_origin_why if a local origin IS the case under test."
        ]
    if scenario.local_origin_why and not local:
        return [
            f"    {side}'s plane origin is {origin!r}, which is not local, but the scenario "
            "says it is (local_origin_why) — drop the note"
        ]
    return []


def _pending(root: Path) -> None:
    """Two memories and a file nobody meant to commit — the shape of the 2026-09-12 incident."""
    for rel, text in (
        ("personas/steward/memory/m.md", "# a memory\n\nbody\n"),
        ("personas/_shared/memory/s.md", "# shared\n\nbody\n"),
        ("uv.lock", "some agent's local uv run left this here\n"),
    ):
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)


def _a_plane_with_work_pending(root: Path) -> None:
    _plane_repo(root)
    _pending(root)


def _a_plane_with_nothing_pending(root: Path) -> None:
    """Nothing staged at all — and so, deliberately, NO breakdown.

    The scenario this feeds declares no `rust_only_lines`, which is what holds charter-app to
    printing nothing here: its stderr is compared to charter's byte for byte, so a breakdown
    over an empty commit — "0 file(s) across 0 directories" — fails it. The absence is tested
    by the ordinary comparison rather than by a rule of its own.
    """
    _plane_repo(root)


def _a_plane_with_one_file_pending(root: Path) -> None:
    _plane_repo(root)
    path = root / "personas" / "steward" / "memory" / "m.md"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("# a memory\n\nbody\n")


def _a_plane_with_work_in_more_directories_than_the_breakdown_lists(root: Path) -> None:
    """Eighteen files across fifteen directories, two of them holding more than one file.

    More than `planegit::BREAKDOWN_ROWS`, so `save` lists twelve and says how many it did not
    — a branch no scenario had ever entered. The uneven counts are what make the ordering
    visible: sorted biggest-first, `notes/d01` and `notes/d02` come before a directory whose
    name sorts ahead of both.
    """
    _plane_repo(root)
    for i in range(1, 15):
        directory = root / "notes" / f"d{i:02d}"
        directory.mkdir(parents=True, exist_ok=True)
        for j in range({1: 3, 2: 2}.get(i, 1)):
            (directory / f"n{j}.md").write_text(f"# note {i}.{j}\n")
    (root / "uv.lock").write_text("some agent's local uv run left this here\n")


def _a_plane_that_is_not_a_repository(root: Path) -> None:
    _pending(root)


def _a_plane_whose_origin_is_on_no_forge_charter_knows(root: Path) -> None:
    bare = _plane_repo(root, origin=None, rewrite=False)
    _git(root.parent, "remote", "add", "origin", f"file://{bare}", cwd=root)
    _pending(root)


#: The credential planted in a memory file below. Spelled once, so the scenario's
#: `never_says` and the file it is written into cannot drift apart.
THE_TOKEN_IN_THAT_MEMORY = "ghp_0123456789abcdefghijklmnopqrstuvwxyz"


def _a_plane_with_a_secret_in_a_memory_file(root: Path) -> None:
    _plane_repo(root)
    store = root / "personas" / "steward" / "memory"
    store.mkdir(parents=True, exist_ok=True)
    (store / "leak.md").write_text(f"# the deploy\n\ntoken: {THE_TOKEN_IN_THAT_MEMORY}\n")


def _a_plane_whose_operator_signs_every_commit(root: Path) -> None:
    """A global `commit.gpgsign = true`, which is the ordinary case the policy exists for.

    What this scenario holds the two implementations to is that the plane still BEHAVES: both
    commit, and both say the same thing. It cannot hold them to more, because a commit that
    reached the signer and failed is rescued by the `--no-gpg-sign` retry either way — so
    "the signer was never asked" needs a signer that records being asked, and that lives in
    `planegit`'s own tests (`the_signer_is_never_asked_even_when_the_operator_signs_every_commit`)
    where the marker can be checked directly.
    """
    side = root.parent
    _plane_repo(root, extra_config=["[commit]", "\tgpgsign = true",
                                    "[tag]", "\tgpgsign = true",
                                    "[gpg]", f"\tprogram = {side}/no-such-signer"])
    _pending(root)


def _a_plane_whose_branch_requires_a_pull_request(root: Path) -> None:
    """The stand-in forge refuses `refs/heads/main` in GitHub's own wording.

    A real `pre-receive` hook, because the rejection IS the evidence: charter never asks a forge
    whether a branch is protected, and guessing it from the branch name is the unearned
    diagnosis ADR 0009 forbids.
    """
    bare = _plane_repo(root)
    hook = bare / "hooks" / "pre-receive"
    hook.parent.mkdir(parents=True, exist_ok=True)
    hook.write_text(
        "#!/bin/sh\nwhile read _ _ ref; do\n  case \"$ref\" in refs/heads/main)\n"
        "    echo 'remote: error: GH006: Protected branch update failed for "
        "refs/heads/main.' >&2\n    exit 1;; esac\ndone\nexit 0\n"
    )
    hook.chmod(0o755)
    _pending(root)


def _a_plane_and_a_clone_with_no_policy(root: Path) -> None:
    side = root.parent
    _identity(side)
    _git(side, "init", "-q", "-b", "main", ".", cwd=root)
    _git(side, "remote", "add", "origin", "https://github.com/acme/plane.git", cwd=root)
    clone = root / "workspaces" / "alpha" / "widget"
    clone.mkdir(parents=True, exist_ok=True)
    _git(side, "init", "-q", "-b", "main", ".", cwd=clone)
    _git(side, "remote", "add", "origin", "git@gitlab.com:acme/widget.git", cwd=clone)


def _a_clone_on_a_forge_charter_cannot_place(root: Path) -> None:
    side = root.parent
    _identity(side)
    _git(side, "init", "-q", "-b", "main", ".", cwd=root)
    _git(side, "remote", "add", "origin", "https://github.com/acme/plane.git", cwd=root)
    clone = root / "workspaces" / "alpha" / "widget"
    clone.mkdir(parents=True, exist_ok=True)
    _git(side, "init", "-q", "-b", "main", ".", cwd=clone)
    _git(side, "remote", "add", "origin", "https://evil.example/acme/widget.git", cwd=clone)


def _plane_facts(root: Path) -> str:
    """What the plane's repository and its stand-in forge say — everything about the commit
    except when it was made, which is the one thing the two sides cannot share."""

    def ask(where: Path, *args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(where), *args], capture_output=True, text=True,
            env={"PATH": "/usr/bin:/bin", "HOME": "/nonexistent", "GIT_CONFIG_NOSYSTEM": "1"},
        ).stdout

    out = [
        f"branch: {ask(root, 'symbolic-ref', '-q', 'HEAD')}",
        f"tree: {ask(root, 'rev-parse', 'HEAD^{tree}')}",
        f"subject: {ask(root, 'log', '-1', '--format=%s')}",
        f"committed:\n{ask(root, 'show', '--name-only', '--format=', 'HEAD')}",
        f"status:\n{ask(root, 'status', '--porcelain')}",
        f"config:\n{ask(root, 'config', '--local', '--list')}",
    ]
    clone = root / "workspaces" / "alpha" / "widget"
    if (clone / ".git").is_dir():
        out.append(f"clone config:\n{ask(clone, 'config', '--local', '--list')}")
    bare = root.parent / "forge" / "acme" / "plane.git"
    if bare.is_dir():
        refs = []
        for ref in ask(bare, "for-each-ref", "--format=%(refname)").split():
            # A `charter/<sha>` branch is named after a commit the two sides cannot share.
            named = re.sub(r"charter/[0-9a-f]{7,}$", "charter/<sha>", ref)
            refs.append(f"{named} tree {ask(bare, 'rev-parse', ref + '^{tree}').strip()} "
                        f"subject {ask(bare, 'log', '-1', '--format=%s', ref).strip()}")
        out.append("remote:\n" + "\n".join(sorted(refs)))
    # The two copies live under `python/` and `rust/`, and a scenario whose `origin` is a
    # `file://` path therefore records a different absolute URL on each side. That is the
    # harness's own doing, so the SIDE is blanked and every other character still has to match.
    return re.sub(r"/(?:python|rust)/", "/<side>/", "\n".join(out))


#: What the HARNESS itself makes differ, not the implementations: the plane copies live in
#: directories named after their side, and the two commits are made a committer date apart
#: because the Rust git runner clears the environment this harness pins the date in — so the
#: sha in the commit line, and the `charter/<sha>` branch named after it, differ too.
SAVE_MASKS = [
    (r"\S*/(?:python|rust)/plane", "the two plane copies are in differently named directories"),
    (r"Committed [0-9a-f]{7,} in",
     "the two commits carry different committer dates, so their shas differ"),
    (r"charter/[0-9a-f]{7,}", "the pull-request branch is named after that sha"),
]

#: The block the Rust `save` prints and charter does not: what is about to be committed, by
#: directory. charter prints only the count, afterwards — and a count is not a description
#: (`crates/charter-core/src/planegit.rs` carries the incident this comes from).
#:
#: These patterns only say WHICH lines are charter-app's alone. What they SAID is
#: `rust_only_block` below, which every scenario setting these must declare — deleting the
#: breakdown unread is how it stayed outside the differential for a whole milestone (M2.15).
SAVE_BREAKDOWN = [
    (r"^! charter save commits everything pending in ",
     "the headline of the Rust save's pre-commit breakdown"),
    (r"^• +\d+ {2}", "one directory row of that breakdown"),
]

#: The same, for a plane with more directories changed than the breakdown lists rows. The tail
#: is a separate pattern because a pattern that matches nothing fails the scenario: only a
#: plane with more than `BREAKDOWN_ROWS` directories prints it.
SAVE_BREAKDOWN_TRUNCATED = [
    *SAVE_BREAKDOWN,
    (r"^• +… and \d+ more directories\. The whole list: ",
     "the breakdown's tail, naming how many directories it did not list"),
]

#: What that block says for `_pending`'s three files — two memories and the stray `uv.lock`
#: that is the 2026-09-12 incident's own shape. A file at the top of the plane is named rather
#: than counted under a bare `.`, because that stray file is the whole reason this block
#: exists; the rows are biggest-first and then by name, so with three ones it is by name.
BREAKDOWN_OF_THE_PENDING_WORK = """\
! charter save commits everything pending in <masked> — 3 file(s) across 3 directories, not only what you meant:
•       1  (the plane root)
•       1  personas/_shared/memory
•       1  personas/steward/memory
"""

#: One file in one directory: the smallest breakdown there is, and the one that shows the
#: headline's counts are counts rather than the length of a list that happens to agree.
BREAKDOWN_OF_ONE_FILE = """\
! charter save commits everything pending in <masked> — 1 file(s) across 1 directories, not only what you meant:
•       1  personas/steward/memory
"""

#: Eighteen files across fifteen directories. Twelve rows and then the tail, which is the
#: branch nothing had ever run: `BREAKDOWN_ROWS` is 12 and every scenario until now changed
#: three directories. Ordering is load-bearing here — `notes/d01` (3 files) and `notes/d02`
#: (2) come before `(the plane root)` although the name sorts first, and the twelve that ARE
#: listed are the twelve biggest rather than the first twelve found.
BREAKDOWN_OF_MORE_DIRECTORIES_THAN_IT_LISTS = """\
! charter save commits everything pending in <masked> — 18 file(s) across 15 directories, not only what you meant:
•       3  notes/d01
•       2  notes/d02
•       1  (the plane root)
•       1  notes/d03
•       1  notes/d04
•       1  notes/d05
•       1  notes/d06
•       1  notes/d07
•       1  notes/d08
•       1  notes/d09
•       1  notes/d10
•       1  notes/d11
•   … and 3 more directories. The whole list: git -C <masked> diff --cached --name-only
"""

#: The plane's own `.git`, whose index, reflogs and object timestamps no two runs share.
#: `_plane_facts` compares what it SAYS instead.
PLANE_GIT = {
    ".git": "compared through `facts`: the index and the reflogs carry timestamps and inodes "
            "no two runs share, and the two commits carry different committer dates",
    "workspaces/alpha/widget/.git": "the same, for the clone `git-policy` acts on",
}

#: The record of a push that did not land on the branch HEAD is on. It carries the commit it is
#: about and the moment it was written, and the two sides share neither.
PUSH_RECORD = ("the record names the commit it is about and the time it was written, and the "
               "two sides' commits differ; the branch it points at is compared through `facts`")

#: Where a push lands, which is by definition not inside the tree being pushed.
PUSHED_TO = {
    "forge/acme/plane.git/": "the bare repository standing in for the forge — this is where "
                             "`save`'s push is supposed to land, and a scenario in which "
                             "nothing lands there has stopped testing the push",
}

SAVE_SCENARIOS = [
    Scenario(
        name="save-commits-everything-pending-and-pushes-it",
        plane="daily",
        setup=_a_plane_with_work_pending,
        python=["save", "one save"],
        pins_the_clock=False,
        stderr_mask=SAVE_MASKS,
        rust_only_lines=SAVE_BREAKDOWN,
        rust_only_block=BREAKDOWN_OF_THE_PENDING_WORK,
        ignore=PLANE_GIT,
        facts=_plane_facts,
        writes_outside=PUSHED_TO,
    ),
    Scenario(
        name="save-with-no-push-commits-and-leaves-the-remote-where-it-was",
        plane="daily",
        setup=_a_plane_with_work_pending,
        python=["save", "one save", "--no-push"],
        pins_the_clock=False,
        stderr_mask=SAVE_MASKS,
        rust_only_lines=SAVE_BREAKDOWN,
        rust_only_block=BREAKDOWN_OF_THE_PENDING_WORK,
        ignore=PLANE_GIT,
        facts=_plane_facts,
    ),
    Scenario(
        name="save-with-one-file-pending-names-the-one-directory",
        plane="daily",
        setup=_a_plane_with_one_file_pending,
        python=["save", "one save"],
        pins_the_clock=False,
        stderr_mask=SAVE_MASKS,
        rust_only_lines=SAVE_BREAKDOWN,
        rust_only_block=BREAKDOWN_OF_ONE_FILE,
        ignore=PLANE_GIT,
        facts=_plane_facts,
        writes_outside=PUSHED_TO,
    ),
    Scenario(
        name="save-with-more-directories-pending-than-the-breakdown-lists",
        plane="daily",
        setup=_a_plane_with_work_in_more_directories_than_the_breakdown_lists,
        python=["save", "one save"],
        pins_the_clock=False,
        stderr_mask=SAVE_MASKS,
        rust_only_lines=SAVE_BREAKDOWN_TRUNCATED,
        rust_only_block=BREAKDOWN_OF_MORE_DIRECTORIES_THAN_IT_LISTS,
        ignore=PLANE_GIT,
        facts=_plane_facts,
        writes_outside=PUSHED_TO,
    ),
    Scenario(
        # No `rust_only_lines`, on purpose: charter-app prints no breakdown when there is
        # nothing staged, and with nothing declared its stderr is compared to charter's byte
        # for byte. A breakdown over an empty commit fails here.
        name="save-on-a-clean-tree-commits-nothing",
        plane="daily",
        setup=_a_plane_with_nothing_pending,
        python=["save"],
        pins_the_clock=False,
        ignore=PLANE_GIT,
        facts=_plane_facts,
    ),
    Scenario(
        name="save-commits-under-a-global-commit-gpgsign",
        plane="daily",
        setup=_a_plane_whose_operator_signs_every_commit,
        python=["save", "one save"],
        pins_the_clock=False,
        stderr_mask=SAVE_MASKS,
        rust_only_lines=SAVE_BREAKDOWN,
        rust_only_block=BREAKDOWN_OF_THE_PENDING_WORK,
        ignore=PLANE_GIT,
        facts=_plane_facts,
        writes_outside=PUSHED_TO,
    ),
    Scenario(
        name="save-whose-origin-is-on-no-forge-charter-knows-commits-locally",
        plane="daily",
        setup=_a_plane_whose_origin_is_on_no_forge_charter_knows,
        python=["save", "one save"],
        pins_the_clock=False,
        local_origin_why="a local origin IS the case under test: charter cannot place it on "
                         "any forge, so it commits and says it has nowhere to push. This is "
                         "the ONE scenario allowed to read that way — in every other one it "
                         "means the insteadOf rewrite swallowed the origin and both sides are "
                         "agreeing about a code path neither entered (see _forge_trap)",
        stderr_mask=SAVE_MASKS,
        rust_only_lines=SAVE_BREAKDOWN,
        rust_only_block=BREAKDOWN_OF_THE_PENDING_WORK,
        ignore=PLANE_GIT,
        facts=_plane_facts,
    ),
    Scenario(
        name="save-refuses-a-secret-shaped-value-in-a-memory-file",
        plane="daily",
        setup=_a_plane_with_a_secret_in_a_memory_file,
        python=["save"],
        pins_the_clock=False,
        refusal="Refusing to save — a secret-shaped value in a memory/ref file:",
        # The refusal names the FILE and the kind. It reads the file to decide, so the one
        # thing it must not do is repeat what it found there.
        never_says=(THE_TOKEN_IN_THAT_MEMORY,),
        ignore=PLANE_GIT,
        facts=_plane_facts,
    ),
    Scenario(
        name="save-on-a-plane-that-is-not-a-git-repository",
        plane="daily",
        setup=_a_plane_that_is_not_a_repository,
        python=["save"],
        pins_the_clock=False,
        refusal="is not a git repository, so there is nothing to commit to.",
    ),
    Scenario(
        name="save-onto-a-branch-that-requires-a-pull-request-opens-one",
        plane="daily",
        setup=_a_plane_whose_branch_requires_a_pull_request,
        python=["save", "one save"],
        pins_the_clock=False,
        stderr_mask=SAVE_MASKS,
        rust_only_lines=SAVE_BREAKDOWN,
        rust_only_block=BREAKDOWN_OF_THE_PENDING_WORK,
        ignore={**PLANE_GIT, ".charter/plane-push.json": PUSH_RECORD},
        facts=_plane_facts,
        writes_outside=PUSHED_TO,
    ),
    Scenario(
        name="git-policy-reports-the-plane-and-a-clone-that-are-not-token-only",
        plane="daily",
        setup=_a_plane_and_a_clone_with_no_policy,
        python=["git-policy"],
        pins_the_clock=False,
        ignore=PLANE_GIT,
        facts=_plane_facts,
    ),
    Scenario(
        name="git-policy-apply-writes-the-token-only-policy-per-forge",
        plane="daily",
        setup=_a_plane_and_a_clone_with_no_policy,
        python=["git-policy", "--apply"],
        pins_the_clock=False,
        ignore=PLANE_GIT,
        facts=_plane_facts,
    ),
    Scenario(
        name="git-policy-names-a-clone-whose-forge-it-cannot-place-rather-than-guessing",
        plane="daily",
        setup=_a_clone_on_a_forge_charter_cannot_place,
        python=["git-policy"],
        pins_the_clock=False,
        ignore=PLANE_GIT,
        facts=_plane_facts,
    ),
]


# M2.16: standing in a linked worktree of the plane, every command names the same plane
# --------------------------------------------------------------------------------------------
#
# `charter.toml` is a TRACKED file, so a worktree cut from a committed plane gets its own copy
# checked out and reads as a control plane of its own. Python has one resolver for this and
# always has (`charter/root.py:find_root` → `_plane_of` → the MAIN working tree, which is what
# `config.ROOT` is built from, for every command Python has). charter-app had two: `save` and
# `git-policy` asked a `command_root` that took that step and everything else asked a `resolve`
# that did not — so standing here, `charter save` named the plane and `charter ws remember`
# wrote a memory into the worktree, which `git worktree remove` deletes. M2.16 made it one
# walk; these two scenarios are what stops it becoming two again.
#
# **The worktree is at `<plane>/sandbox`, not under `workspaces/<ws>/.worktrees/`.** charter's
# own layout puts worktrees inside a workspace, and `outermost` already hops out of an
# enclosing plane's `workspaces/` — so in that layout both resolvers landed on the plane
# anyway and the difference is invisible. A worktree anywhere else is the case that diverged,
# and `git worktree add` will put one wherever it is told.
#
# **`$CHARTER_ROOT` is UNSET here, and these are the only scenarios in this suite that unset
# it.** `_env` pins it for every other one, which pins the plane before either resolver walks
# — so the walk this section is about had never been run by the differential at all.

#: The two `.git`s that name absolute paths under the side's own directory: the plane's, which
#: also carries the index and reflogs no two runs share, and the worktree's, which is a FILE
#: reading `gitdir: <side>/plane/.git/worktrees/sandbox`.
WORKTREE_GIT = {
    ".git": "the index, the reflogs and the worktree registry carry timestamps, inodes and "
            "absolute paths under the side's own directory",
    "sandbox/.git": "a linked worktree's `.git` is a file naming the main tree by absolute "
                    "path, which differs by side",
}


def _a_plane_and_a_worktree_cut_from_it(root: Path) -> None:
    """The plane committed, and `git worktree add sandbox` standing beside it.

    `charter.toml` is committed here — that is the whole shape: it is tracked in a real plane,
    so the checkout into `sandbox/` carries it and the worktree looks like a plane.
    """
    side = root.parent
    _identity(side)
    _git(side, "init", "-q", "-b", "main", ".", cwd=root)
    (root / ".gitignore").write_text(".charter/\n")
    _git(side, "add", "-A", cwd=root)
    _git(side, "commit", "-q", "-m", "the plane", cwd=root)
    _git(side, "worktree", "add", "-q", "-b", "feature", "sandbox", cwd=root)


#: The one `.git` inside the plane copy. The worktree is BESIDE the plane here, not under it,
#: so its own `.git` is not part of the tree comparison at all — the containment check reads
#: it before and after instead, which is what a file outside the plane gets.
BARE_WORKTREE_GIT = {
    ".git": WORKTREE_GIT[".git"],
}


def _a_plane_whose_marker_was_never_committed_and_a_worktree(root: Path) -> None:
    """The plane committed WITHOUT its `charter.toml`, and `git worktree add ../sandbox`.

    `charter init` writes the marker and never stages it, so this — not the committed marker
    above — is what a worktree cut from `main` actually looks like. `git rm --cached` rather
    than a `.gitignore` entry, because that is the state `init` leaves: the file is untracked
    and still shows up in `git status`, which is how an operator meets it.

    Only the plane's OWN marker is unstaged. The `daily` fixture carries a second one deep
    under `workspaces/alpha/tool/`, which stays committed — it is below the caller, never
    above, so no walk can reach it, and taking it out would change what the fixture is.
    """
    side = root.parent
    _identity(side)
    _git(side, "init", "-q", "-b", "main", ".", cwd=root)
    (root / ".gitignore").write_text(".charter/\n")
    _git(side, "add", "-A", cwd=root)
    _git(side, "rm", "--cached", "-q", "charter.toml", cwd=root)
    _git(side, "commit", "-q", "-m", "the plane, with its marker never staged", cwd=root)
    _git(side, "worktree", "add", "-q", "-b", "feature", "../sandbox", cwd=root)
    if (side / "sandbox" / "charter.toml").exists():
        raise SystemExit(
            "setup: the worktree carries a charter.toml, so this scenario would exercise the "
            "first walk and say nothing about the second"
        )


WORKTREE_SCENARIOS = [
    Scenario(
        # The proof. Before M2.16 charter-app wrote the memory into `sandbox/workspaces/…`
        # and charter wrote it into `workspaces/…`, so the tree comparison reports it twice:
        # a file only python has, and a file only rust has.
        name="remember-from-a-worktree-cut-from-the-plane-writes-to-the-plane",
        plane="daily",
        setup=_a_plane_and_a_worktree_cut_from_it,
        python=["workspace", "remember", "The importer drops rows over 4 MB", "-w", "alpha",
                "--no-sync"],
        cwd="sandbox",
        env={"CHARTER_ROOT": None},
        ignore=WORKTREE_GIT,
    ),
    Scenario(
        # The other half: `save`, from the same directory, resolving through the same walk.
        # Its refusal is only reachable when the plane it resolved is the one the caller is
        # NOT standing in, so a `save` that stopped taking the redirect would stop refusing —
        # and the words are compared byte for byte, not just searched for.
        name="save-from-a-worktree-cut-from-the-plane-refuses-and-names-both-trees",
        plane="daily",
        setup=_a_plane_and_a_worktree_cut_from_it,
        python=["save", "one save"],
        cwd="sandbox",
        env={"CHARTER_ROOT": None},
        pins_the_clock=False,
        refusal="a linked worktree of it. Committing every change in a tree you are not in",
        same_stderr=True,
        stderr_mask=SAVE_MASKS,
        ignore=WORKTREE_GIT,
    ),
    # M2.23: the SECOND walk — a worktree that carries no `charter.toml` at all
    # ----------------------------------------------------------------------------------------
    #
    # The two above are the case where the worktree HAS the marker, because the plane committed
    # it. This is the other one, and it is the common shape rather than the exotic: `charter
    # init` writes `charter.toml` and never stages it, so a worktree cut from `main` does not
    # contain one. `plane_of` cannot fire here — the first walk found no marker to redirect —
    # and before M2.23 the Rust resolver simply stopped, so following charter's own `enter:`
    # line landed a session in a plane-less directory with no personas and no vault, writing
    # memory where `git worktree remove --force` deletes it, while `doctor` reported green.
    #
    # **The worktree is `../sandbox`, BESIDE the plane copy rather than inside it.** A worktree
    # under the plane is answered by the first walk — the marker above it — and a scenario
    # placed there would say nothing about the second at all.
    Scenario(
        # The write proof, and the same shape as the M2.16 one above: before the fix the Rust
        # side resolved no plane, exited non-zero and wrote nothing, so the memory is a file
        # only python has.
        name="remember-from-a-worktree-whose-plane-was-never-committed-writes-to-the-plane",
        plane="daily",
        setup=_a_plane_whose_marker_was_never_committed_and_a_worktree,
        python=["workspace", "remember", "The importer drops rows over 4 MB", "-w", "alpha",
                "--no-sync"],
        cwd="../sandbox",
        env={"CHARTER_ROOT": None},
        ignore=BARE_WORKTREE_GIT,
    ),
    Scenario(
        # `reinit` resolves through `place` — `find_root_or_cwd` — which is the SAME walk in
        # Python and was a second one here. Its failure mode is the loudest of all: with no
        # plane resolved, `place` falls back to the working directory, so `reinit` scaffolds a
        # second plane into the worktree — a write outside this side's plane copy, which the
        # containment check reports by name.
        name="reinit-from-a-worktree-whose-plane-was-never-committed-heals-the-plane",
        plane="daily",
        setup=_both(_opencode_already_installed,
                    _a_plane_whose_marker_was_never_committed_and_a_worktree),
        python=["reinit"],
        cwd="../sandbox",
        env={"CHARTER_ROOT": None},
        pins_the_clock=False,
        python_writes_outside={OPENCODE_CONTEXT: OPENCODE_CONTEXT_WHY},
        ignore=BARE_WORKTREE_GIT,
    ),
]


# --------------------------------------------------------------------------------------- #
# M2.9: the two resolution ladders, one rung at a time                                      #
# --------------------------------------------------------------------------------------- #
#: The pane id the ladder scenarios key their terminal pointer on. `_env` sets no pane
#: variable and `_run` gives nothing a tty, so that rung is off in every other scenario.
PANE = "fixture-pane-1"

#: Why `workspace current` and `persona current` say more on the Python side. Their STDOUT —
#: the resolved name, alone, which is what a script reads — must match byte for byte; the
#: sentence explaining which rung decided, whether the workspace is live, the session lock
#: and the vision is the command presentation layer, and porting it is not what M2.9 is.
CURRENT_EXPLAINS_ITSELF = (
    "charter's `current` prints the name on stdout and then explains it on stderr — the rung "
    "that decided, LIVE/LOCAL, the session lock, the vision. The name is what this compares; "
    "the explanation is the presentation layer."
)

#: Every rung of the workspace ladder that lives on disk, top to bottom. Each names a
#: DIFFERENT workspace, because a rung naming the same one as the rung below it proves
#: nothing about which of the two decided.
WORKSPACE_RUNGS = ("session", "frame", "terminal", "declared", "plane")

#: The same for personas. `steward` and `devops` are personas the `daily` plane HAS, and the
#: bottom two rungs need that: a committed rung naming a persona that does not exist resolves
#: to nothing, while the pointers above it deliberately still win with a name nothing defines.
PERSONA_RUNGS = ("session", "terminal", "active", "plane", "committed")


def _workspace_ladder(*absent: str):
    """A setup that plants every workspace rung except the ones named.

    Removing rungs from the TOP down, one scenario per rung, is the only way to prove
    precedence: a test that sets one rung and reads it back cannot tell "this rung decided"
    from "this rung and the four below it happen to agree".
    """
    unknown = set(absent) - set(WORKSPACE_RUNGS)
    assert not unknown, f"no such rung: {unknown}"

    def plant(root: Path) -> None:
        (root / ".charter" / "sessions").mkdir(parents=True, exist_ok=True)
        (root / ".charter" / "terminals").mkdir(parents=True, exist_ok=True)
        (root / ".charter" / "frame" / SESSION).mkdir(parents=True, exist_ok=True)
        # The tree rung: a workspace with something inside it, because `workspaces/<ws>` on
        # its own is the container and not a tree.
        (root / "workspaces" / "from-cwd" / "repo").mkdir(parents=True, exist_ok=True)
        (root / "workspaces" / "beta" / "repo").mkdir(parents=True, exist_ok=True)
        files = {
            "session": (root / ".charter" / "sessions" / f"{SESSION}.workspace", "from-session"),
            "frame": (root / ".charter" / "frame" / SESSION / "workspace", "from-frame"),
            "terminal": (root / ".charter" / "terminals" / f"{PANE}.workspace", "from-terminal"),
            "declared": (root / "workspaces" / ".default", "from-declared"),
        }
        for rung, (path, value) in files.items():
            if rung in absent:
                path.unlink(missing_ok=True)
            else:
                path.write_text(value + "\n")
        if "plane" not in absent:
            toml = root / "charter.toml"
            toml.write_text(toml.read_text() + '\n[workspace]\ndefault = "from-plane"\n')

    return plant


def _persona_ladder(*absent: str):
    """The same for the persona ladder — `charter/persona.py:1379`."""
    unknown = set(absent) - set(PERSONA_RUNGS)
    assert not unknown, f"no such rung: {unknown}"

    def plant(root: Path) -> None:
        (root / ".charter" / "sessions").mkdir(parents=True, exist_ok=True)
        (root / ".charter" / "terminals").mkdir(parents=True, exist_ok=True)
        files = {
            "session": (root / ".charter" / "sessions" / f"{SESSION}.persona", "from-session"),
            "terminal": (root / ".charter" / "terminals" / f"{PANE}.persona", "from-terminal"),
            "active": (root / ".charter" / "active-persona", "from-active"),
            # The two committed rungs name personas the plane HAS, because a committed rung
            # that names one it does not resolves to nothing at all.
            "committed": (root / "personas" / ".default", "devops"),
        }
        for rung, (path, value) in files.items():
            if rung in absent:
                path.unlink(missing_ok=True)
            else:
                path.write_text(value + "\n")
        if "plane" in absent:
            toml = root / "charter.toml"
            toml.write_text(toml.read_text().replace('[persona]\ndefault = "steward"\n', ""))

    return plant


def _workspace_rung(name: str, *, absent: tuple = (), env: dict | None = None, **kw) -> Scenario:
    """One `workspace current`, with the named rungs taken away.

    The pane id is on by default because the terminal pointer is a rung; a scenario that
    wants it off says so with `env={"TERM_SESSION_ID": None}` rather than by omission.
    """
    return Scenario(
        name=name,
        plane="daily",
        python=["workspace", "current"],
        pins_the_clock=False,
        setup=_workspace_ladder(*absent),
        env={"TERM_SESSION_ID": PANE, **(env or {})},
        stderr_differs=CURRENT_EXPLAINS_ITSELF,
        **kw,
    )


def _persona_rung(name: str, *, absent: tuple = (), env: dict | None = None, **kw) -> Scenario:
    """One `persona current`, with the named rungs taken away."""
    return Scenario(
        name=name,
        plane="daily",
        python=["persona", "current"],
        pins_the_clock=False,
        setup=_persona_ladder(*absent),
        env={"TERM_SESSION_ID": PANE, **(env or {})},
        stderr_differs=CURRENT_EXPLAINS_ITSELF,
        **kw,
    )


LADDER_SCENARIOS = [
    # --- the workspace ladder, rung by rung -------------------------------------------- #
    _workspace_rung(
        "workspace-the-environment-outranks-the-tree-and-every-pointer",
        # `workspace current` takes no `-w` in either implementation, so the top rung is
        # proved where it is actually used: on the commands that WRITE, below.
        env={"CHARTER_WORKSPACE": "from-env"},
        cwd="workspaces/from-cwd/repo",
    ),
    _workspace_rung(
        "workspace-a-blank-environment-is-not-a-rung-and-the-tree-decides",
        # charter#1055. Taken as a name, a whitespace `$CHARTER_WORKSPACE` hid every rung
        # below it and `source` reported the variable as the operator's own choice.
        env={"CHARTER_WORKSPACE": " \t "},
        cwd="workspaces/from-cwd/repo",
    ),
    _workspace_rung(
        "workspace-the-tree-you-stand-in-outranks-every-pointer",
        cwd="workspaces/from-cwd/repo",
    ),
    _workspace_rung(
        "workspace-the-workspace-directory-itself-is-a-container-and-not-a-tree",
        cwd="workspaces/from-cwd",
    ),
    _workspace_rung("workspace-with-no-tree-the-sessions-own-pointer-decides"),
    _workspace_rung(
        "workspace-the-frames-launch-record-is-a-rung-in-python-and-not-here",
        absent=("session",),
        # **The one rung this port deliberately does not have.** `docs/plane-format.md`:
        # `.charter/frame/**` is the tmux frame's, and the app replaces that frame rather
        # than reading its state — so nothing on this side can ever write this record, and a
        # rung nothing can set is a rung nobody can reason about. Python answers `from-frame`
        # and this answers the rung below it.
        stdout_differs=(
            "the frame's launch record is `.charter/frame/<fid>/workspace`, which "
            "docs/plane-format.md rules is the tmux frame's and the app neither reads nor "
            "writes. Python takes it as the rung below the session pointer; this port has no "
            "such rung and falls to the terminal pointer."
        ),
    ),
    _workspace_rung(
        "workspace-with-no-session-pointer-the-terminals-decides",
        absent=("session", "frame"),
    ),
    _workspace_rung(
        "workspace-with-no-pane-id-the-terminal-pointer-is-not-read",
        absent=("session", "frame"),
        # No pane variable and no tty: the id is `None`, so the pointer that IS there is not
        # this shell's to read. Costing only a convenience is the deliberate trade — an id
        # that is wrong in the sharing direction is worse than no id.
        env={"TERM_SESSION_ID": None},
    ),
    _workspace_rung(
        "workspace-with-neither-pointer-the-nominated-default-decides",
        absent=("session", "frame", "terminal"),
    ),
    _workspace_rung(
        "workspace-with-nothing-nominated-the-planes-declared-default-decides",
        absent=("session", "frame", "terminal", "declared"),
    ),
    _workspace_rung(
        "workspace-with-a-plane-that-declares-nothing-the-answer-is-the-built-in",
        absent=WORKSPACE_RUNGS,
    ),
    # --- the persona ladder, rung by rung ---------------------------------------------- #
    _persona_rung(
        "persona-the-environment-outranks-every-pointer",
        env={"CHARTER_PERSONA": "from-env"},
    ),
    _persona_rung(
        "persona-a-blank-environment-is-not-a-rung",
        # charter#1048, the same defect one noun over.
        env={"CHARTER_PERSONA": "  "},
    ),
    _persona_rung("persona-with-no-environment-the-sessions-own-pointer-decides"),
    _persona_rung(
        "persona-with-no-session-pointer-the-terminals-decides",
        absent=("session",),
    ),
    _persona_rung(
        "persona-with-neither-pointer-the-plane-wide-active-file-decides",
        absent=("session", "terminal"),
    ),
    _persona_rung(
        "persona-with-nothing-selected-the-planes-declared-front-door-decides",
        absent=("session", "terminal", "active"),
    ),
    _persona_rung(
        "persona-with-nothing-declared-the-legacy-committed-default-decides",
        absent=("session", "terminal", "active", "plane"),
    ),
    _persona_rung(
        "persona-a-plane-with-no-front-door-has-none-and-says-so",
        absent=PERSONA_RUNGS,
    ),
    # --- and the same ladders on the commands that act ---------------------------------- #
    Scenario(
        name="recall-with-no-flags-at-all-is-how-a-harness-calls-it",
        # **The command M2.9 exists for.** Before it this exited 2 with "recall needs -w
        # <workspace> ... this charter does not resolve the active workspace", and a harness
        # calls it exactly like this at session start.
        plane="daily",
        python=["recall"],
    ),
    Scenario(
        name="recall-with-no-flags-on-a-plane-with-no-front-door",
        # A persona rung that answers nothing is not a refusal: `recall.sources` skips a
        # scope with no owner, and the workspace and shared bases are still searched.
        plane="daily",
        setup=_persona_ladder(*PERSONA_RUNGS),
        python=["recall"],
    ),
    Scenario(
        name="recall-with-no-flags-resolves-the-workspace-from-the-tree-it-runs-in",
        plane="daily",
        setup=_workspace_ladder("frame"),
        cwd="workspaces/beta/repo",
        python=["recall"],
    ),
    Scenario(
        name="persona-recall-with-no-name-resolves-the-active-persona",
        plane="daily",
        python=["persona", "recall"],
        pins_the_clock=False,
    ),
    Scenario(
        name="persona-remember-with-one-word-takes-it-as-the-fact-not-the-persona",
        plane="daily",
        python=["persona", "remember", "The importer drops rows over 4 MB", "--no-sync"],
    ),
    Scenario(
        name="workspace-recall-with-no-w-reads-the-workspace-the-pointer-names",
        plane="daily",
        pins_the_clock=False,
        python=["workspace", "recall"],
    ),
    Scenario(
        name="workspace-forget-with-no-w-names-the-workspace-it-resolved",
        plane="daily",
        python=["workspace", "forget", "nope"],
        refusal="no memory 'nope' in workspace 'alpha'",
        same_stderr=True,
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="vision-with-no-w-writes-where-the-environment-says",
        plane="daily",
        python=["workspace", "vision", "Ship the widget"],
        env={"CHARTER_WORKSPACE": "beta"},
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="vision-with-w-outranks-an-environment-naming-another-workspace",
        plane="daily",
        python=["workspace", "vision", "Ship the widget", "-w", "alpha"],
        env={"CHARTER_WORKSPACE": "beta"},
    ),
    Scenario(
        name="workspace-remember-with-no-w-writes-where-the-sessions-pointer-says",
        # The `daily` fixture's own `.charter/sessions/fixture-session-1.workspace` says
        # `alpha`, so this is the rung a harness actually lands on, unplanted.
        plane="daily",
        python=["workspace", "remember", "The importer drops rows over 4 MB", "--no-sync"],
    ),
    Scenario(
        stderr_differs=CONFIRMS_ON_STDERR,
        name="todo-with-no-w-writes-into-the-tree-you-are-standing-in",
        # The session pointer says `from-session` and the tree says `beta`; the tree outranks
        # it, because you cannot be in two directories at once.
        plane="daily",
        python=["ws", "todo", "Cut the release"],
        setup=_workspace_ladder("frame"),
        cwd="workspaces/beta/repo",
    ),
]

# --------------------------------------------------------------------------------------------
# `charter news`: the entries themselves, rendered
# --------------------------------------------------------------------------------------------
#
# **These scenarios are the tie between two copies of one corpus.** charter ships `docs/news/`
# in its wheel; charter-app compiles `crates/charter-core/news/` into its binary. Until news is
# authored in one place there are two copies, and `charter/news.py`'s own docstring says what a
# second copy does: *it drifts from the binary, invisibly and in both directions.*
#
# Nothing inside either implementation can see that drift. This can: the oracle is pinned to the
# charter commit the corpus was taken from, and `news --for <version>` renders every entry of a
# version through every rule there is — the order, the `security:` label, the elision and its
# arithmetic, the percent-encoded filename, the tag a link points at. One scenario per version,
# generated from the vendored directory rather than listed, so a version added to the corpus is
# compared from the day it lands and a version that quietly vanishes from it is not silently
# uncompared.
#
# The version list is read from the DIRECTORY and not from the oracle, because this file has to
# keep working whether or not charter is importable in its own process (`_declare_a_profile_per_
# command_word` says the same thing about the other direction).
#
# **That leaves one hole, and `check_corpus` closes it.** A scenario generated from the vendored
# directory cannot see a file DELETED from that directory: no scenario is generated for a version
# that is not there, and every scenario that is generated passes. So the two corpora are compared
# as file lists, once, before any scenario runs — which is also the cheapest possible failure for
# the most likely kind of drift, and the one that names the files rather than a rendered diff.

NEWS_DIR = REPO / "crates" / "charter-core" / "news"

#: `charter news --for` REFUSES this version: six of its entries quote a headline, and the
#: release gate reports that rather than publishing the quotes inside the heading (charter #902).
#: Its own scenario below, because the generated ones expect an exit 0 — and if a future version
#: joins it, that version's generated scenario goes red, which is exactly what a release gate
#: catching a new offender should look like.
NEWS_QUOTED_VERSION = "0.56.0"


def check_corpus() -> bool:
    """The vendored `news/` and the oracle's own entries are the same files, byte for byte.

    Asked of the ORACLE as a subprocess, under the interpreter the scenarios run it with, rather
    than imported here — this file keeps working whether or not charter is importable in its own
    process, and the oracle is pinned to the commit the corpus was taken from.

    **Bytes rather than names, and the difference is a whole class of drift.** The per-version
    `news --for` scenarios compare RENDERED BODIES, so a headline or a body that drifts turns one
    of them red. What a rendered body does not carry is the rest of the frontmatter: `check:`,
    `adopt:` and — for an entry that is alone in its version — `lead:`. An entry whose `adopt:`
    line said one thing here and another in charter would render identically in all 27 of them,
    and the only view that prints an `adopt:` line is `charter news --pending`, which has no
    scenario at all (its Python answer depends on the runner). A digest closes that, and closes
    the missing-file case with it: a file nobody vendored is in no scenario to fail.
    """
    said = subprocess.run(
        [sys.executable, "-c",
         "import hashlib;from charter import news;d = news._dir();"
         "print('\\n'.join(f'{p.name} {hashlib.sha256(p.read_bytes()).hexdigest()}'"
         " for p in sorted(d.glob('*.md'))) if d else '')"],
        capture_output=True, text=True,
    )
    if said.returncode != 0:
        print("DIFF news-corpus: the oracle could not list its entries — " + said.stderr.strip())
        return False
    theirs = dict(line.split() for line in said.stdout.splitlines() if line.strip())
    ours = {
        p.name: hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(NEWS_DIR.glob("*.md"))
    }
    if not theirs:
        print("DIFF news-corpus: the oracle ships no entries, so nothing was compared")
        return False
    problems = []
    for name in sorted(set(theirs) - set(ours)):
        problems.append(f"    only charter has: docs/news/{name}")
    for name in sorted(set(ours) - set(theirs)):
        problems.append(f"    only charter-app has: crates/charter-core/news/{name}")
    for name in sorted(set(theirs) & set(ours)):
        if theirs[name] != ours[name]:
            problems.append(f"    differs: {name}")
    print(("ok   " if not problems else "DIFF ") + f"news-corpus ({len(theirs)} entries)")
    for line in problems:
        print(line)
    if problems:
        print("    the two copies of charter's news have drifted; see "
              "crates/charter-core/news/SOURCE")
    return not problems


def _news_versions() -> list[str]:
    """Every version the vendored corpus names, oldest first, with `unreleased` last.

    From the FILENAME rather than the frontmatter: this list only decides which commands to run,
    and both sides then answer from the frontmatter — so a file whose name and `version:` field
    disagree shows up as a scenario that renders nothing on either side, which is a finding.
    """
    seen: dict[str, None] = {}
    for path in sorted(NEWS_DIR.glob("*.md")):
        seen.setdefault(path.name.split("-", 1)[0], None)
    versions = [v for v in seen if v != "unreleased"]
    versions.sort(key=lambda v: [int(part) for part in v.split(".")])
    if "unreleased" in seen:
        versions.append("unreleased")
    return versions


NEWS_SCENARIOS = [
    Scenario(
        name=f"news-for-{version}-renders-the-same-notes",
        plane="minimal",
        python=["news", "--for", version],
        pins_the_clock=False,
    )
    for version in _news_versions()
    if version != NEWS_QUOTED_VERSION
] + [
    Scenario(
        name="news-for-a-version-that-quotes-a-headline-is-refused-before-it-publishes",
        plane="minimal",
        python=["news", "--for", NEWS_QUOTED_VERSION],
        pins_the_clock=False,
        refusal=f"the news for {NEWS_QUOTED_VERSION} quotes a value charter does not unquote:",
        same_stderr=True,
    ),
    Scenario(
        name="news-for-a-version-nothing-shipped",
        plane="minimal",
        python=["news", "--for", "9.9.9"],
        pins_the_clock=False,
        refusal="no news entry for 9.9.9.",
        same_stderr=True,
    ),
    Scenario(
        # 62 entries across three versions, two of them carrying `security:` notes and one a
        # `lead:` — so this compares the ORDER and the label, which `--for` compares inside one
        # version and this compares across three.
        #
        # `--until` is given rather than defaulted on purpose: charter's default is
        # `charter.__version__`, a Python package version, and the Rust binary's default is the
        # newest version its corpus names (`news::shipped_version`). Those are different
        # questions with different answers, and this scenario is about the entries rather than
        # about which of the two is right — the PR argues that.
        #
        # None of these three versions carries a `check:`, which is what makes the two sides'
        # stdout comparable at all: every entry is informational on both, so neither prints an
        # adopt line. A range that did carry one would compare charter's real probe against a
        # Rust CLI that has no `persona lint` to run.
        name="news-range-lists-the-same-entries-in-the-same-order",
        plane="daily",
        python=["news", "--since", "0.60.0", "--until", "0.62.1"],
        pins_the_clock=False,
    ),
    Scenario(
        # The view that is honest about having no baseline. charter never READS the baseline
        # `charter update` stamps (`commands_update.read_baseline` has no caller), so this
        # sentence is what a plane gets whether or not one is recorded — reported upstream, and
        # ported as it stands rather than quietly improved, because an improvement here is a
        # divergence nothing else would catch.
        name="news-with-no-baseline-reports-no-range",
        plane="daily",
        python=["news"],
        pins_the_clock=False,
    ),
]


# --------------------------------------------------------------------------------------- #
# M2.8: handoff, and the workspace verbs that act on a workspace as a whole                 #
# --------------------------------------------------------------------------------------- #
#: A session id with no frame record under it, so Python's `workspace.launch_lock` — the rung
#: this charter deliberately has no record for (`charter_core::active`) — answers nothing. The
#: fixture's own `fixture-session-1` HAS a frame directory, and every scenario about a lock or
#: a pointer would otherwise be comparing that divergence instead of the verb it names.
FRESH_SESSION = "fresh-session-2"

#: Why `charter handoff`'s last refusal differs. Everything in front of it is ported word for
#: word (`same_stderr`); this one is the frame check, and the two implementations have
#: different frames to report on — charter has a tmux frame and this binary has none at all,
#: so it says so and prints the same command to run in a new terminal.
HANDOFF_HAS_NO_FRAME = (
    "charter opens the chat in a background window of its own tmux; this binary has no frame "
    "and no channel into the app that opens one, so it always reaches the frame refusal. Both "
    "print the command to run in a new terminal, and neither writes anything."
)

#: Why `remove`'s unreadable-clone refusal differs. Both refuse, both exit 2 and both name the
#: clone; what follows `could not be read` is git's own sentence, and the two runners quote a
#: different part of it.
REMOVE_QUOTES_GIT = (
    "both refuse and both name the clone; the words after `could not be read` are git's own, "
    "and the two runners quote a different part of them."
)

#: The brief a handoff scenario approves. Two lines, so the first message is never a single
#: word and the size bound is nowhere near.
A_BRIEF = "# Retry the failed webhook deliveries\n\nThe queue is in workspaces/alpha/svc.\n"

#: A credential-shaped brief, in the one spelling both classifiers read as a VALUE rather than
#: a reference. The KIND is what the refusal names; the value never appears in it — which the
#: scenario's `never_says` is what actually checks.
A_BRIEF_WITH_A_SECRET = "# Rotate the key\n\nAPI_KEY=abcdefghij\n"

#: The value inside it, which is the half a refusal may not repeat. Spelled apart from the
#: brief so the assertion cannot silently stop naming the thing the brief carries.
THE_VALUE_IN_THAT_BRIEF = "abcdefghij"


def _a_session_lock(root: Path) -> None:
    """A lock file for :data:`FRESH_SESSION`, so `unlock` has one to release."""
    sessions = root / ".charter" / "sessions"
    sessions.mkdir(parents=True, exist_ok=True)
    lock = sessions / f"{FRESH_SESSION}.lock"
    lock.write_text("alpha\n")
    lock.chmod(0o600)


def _a_clone_charter_cannot_read(root: Path) -> None:
    """A directory under `alpha` that looks like a clone and is not one.

    A `.git` DIRECTORY is what both implementations read as a clone, and an empty one makes
    `git status` fail — which is charter#917's whole case: a status that failed is not a clean
    tree, and what the at-risk list is empty of is what `remove` deletes.
    """
    (root / "workspaces" / "alpha" / "broken" / ".git").mkdir(parents=True)



# M2.24/M2.25: a CHECKOUT inside a workspace
# --------------------------------------------------------------------------------------------
#
# The fixture planes cannot carry one. Git will not track a path inside a `.git` directory, so
# `generate.py` prunes every one of them — which is why `daily`'s `alpha/svc` and `alpha/tool`
# are ordinary directories on disk and no scenario before this one ever reached the code that
# wires a clone. M2.22 shipped a warning fired from a line nothing differential could see.
#
# So the checkout is made HERE, in a scenario's own setup, identically on both sides: `git
# init` in place rather than a clone, because a clone records its origin's ABSOLUTE path and
# that path is `python/…` on one side and `rust/…` on the other.


def _a_checkout_inside_a_workspace(root: Path) -> None:
    """A git checkout at `workspaces/beta/svc`, holding one commit and nothing of charter's.

    `beta` and not `alpha`: `alpha` already holds directories called `svc` and `tool`, and the
    scenario is about a workspace whose clone charter has never wired.
    """
    tree = root / "workspaces" / "beta" / "svc"
    tree.mkdir(parents=True)
    _git(root.parent, "init", "-q", "-b", "main", ".", cwd=tree)
    (tree / "README.md").write_text("# svc\n")
    _git(root.parent, "add", "-A", cwd=tree)
    _git(root.parent, "commit", "-q", "-m", "one", cwd=tree)


def _a_checkout_holding_a_settings_file_of_yours(root: Path) -> None:
    """The same checkout, with a `.claude/settings.json` the operator wrote.

    The one state where charter writes NOTHING into a checkout and hides nothing either:
    naming that path in the block would hide their own untracked file from their own `git
    status`, which is the failure the ownership rule exists to prevent.
    """
    _a_checkout_inside_a_workspace(root)
    claude = root / "workspaces" / "beta" / "svc" / ".claude"
    claude.mkdir()
    (claude / "settings.json").write_text('{"mine": true}\n')


#: The checkout's `.git`: an index carrying inode numbers and mtimes no two runs share. What
#: matters in it — charter's block, and whether anything charter wrote shows in `git status` —
#: is compared through `facts` instead.
CHECKOUT_GIT = {
    "workspaces/beta/svc/.git": "compared through `facts`: the index carries inodes and "
                                "mtimes no two runs share"
}

#: A linked worktree's `.git` is a FILE holding `gitdir: <absolute path>`, which names
#: `python/…` on one side and `rust/…` on the other. Its block — the clone's, which both read
#: — is compared through `facts`.
SIBLING_GIT = {
    **CHECKOUT_GIT,
    "workspaces/beta/svc-wt/.git": "a `gitdir:` pointer holding each side's own absolute path",
}

PIECE_GIT = {
    **CHECKOUT_GIT,
    "workspaces/beta/.worktrees/svc/p1/.git": "a `gitdir:` pointer holding each side's own "
                                              "absolute path",
}


#: The two plane copies live in directories named after their side, so a sentence that names
#: a checkout by its ABSOLUTE path — which charter's `unhidden` and `unlisted` both do, because
#: the path it names is git's own spelling of another tree — differs by that prefix and by
#: nothing else. Everything below the root is still compared byte for byte, which is where the
#: checkout, the file and the exclude that hides it are.
PLANE_COPY_MASK = [
    (r"\S*/(?:python|rust)/plane", "the two plane copies are in differently named directories"),
]


def _a_worktree_of_the_checkout(root: Path, branch: str = "side", at: str = "svc-wt") -> Path:
    """A linked worktree of `workspaces/beta/svc`, inside the same workspace.

    The whole point of `_shared_rels`: git treats `info/` as shared, so this worktree and the
    clone read ONE `info/exclude` — the clone's — and a line either one writes into it hides
    that path in BOTH.
    """
    _a_checkout_inside_a_workspace(root)
    tree = root / "workspaces" / "beta" / "svc"
    where = root.joinpath("workspaces", "beta", *at.split("/"))
    where.parent.mkdir(parents=True, exist_ok=True)
    _git(root.parent, "worktree", "add", "-q", "-b", branch, str(where), cwd=tree)
    return where


def _a_clone_and_a_worktree_of_it(root: Path) -> None:
    _a_worktree_of_the_checkout(root)


def _a_worktree_of_it_holding_a_settings_file_of_yours(root: Path) -> None:
    """charter#1072: the sibling holds an untracked `.claude/settings.json` of the operator's,
    so the line the clone's wire would add for ITS `.claude/settings.json` would hide theirs.

    The line is left out, charter's own file is still written and shows in the clone's own
    `git status`, and the row says whose file stopped it and what clears it.
    """
    where = _a_worktree_of_the_checkout(root)
    (where / ".claude").mkdir()
    (where / ".claude" / "settings.json").write_text('{"mine": true}\n')


def _a_worktree_of_it_holding_your_machine_local_file(root: Path) -> None:
    """The same, for the MACHINE-LOCAL file — the one case where charter withholds a write.

    A file charter cannot hide is one `git add` from being committed into somebody else's
    repository, so the shared settings and the mirrored agents are written and this one is
    not. The plane declares machine-local rules here, because a plane with none generates no
    such file and the state cannot be reached at all.
    """
    (root / ".claude" / "settings.local.json").write_text(
        json.dumps({"permissions": {"deny": ["Bash(rm -rf /)"]}}, indent=2) + "\n"
    )
    where = _a_worktree_of_the_checkout(root)
    (where / ".claude").mkdir()
    (where / ".claude" / "settings.local.json").write_text('{"mine": true}\n')


def _a_piece_under_the_workspaces_worktrees(root: Path) -> None:
    """A PIECE at `workspaces/beta/.worktrees/svc/p1` — the layout `charter wt add` cuts.

    `.worktrees` holds no `.git` of its own and the piece sits two levels below the
    workspace, so nothing that walks the workspace's children finds it: it is reached only by
    asking git to list the repository's worktrees.
    """
    _a_worktree_of_the_checkout(root, branch="p1", at=".worktrees/svc/p1")


def _a_repository_whose_worktrees_git_cannot_list(root: Path) -> None:
    """The clone, a worktree of it, and `.git/worktrees` unreadable (charter#1072).

    Measured on git 2.50.1: `git worktree list` then lists the clone ALONE and exits 0, so
    its answer is no evidence the list is whole — and the worktree it leaves out gets no
    layer, no repair and, until this row existed, no mention.
    """
    _a_worktree_of_the_checkout(root)
    os.chmod(root / "workspaces" / "beta" / "svc" / ".git" / "worktrees", 0)


#: charter keeps the errno a record publish failed with in its own state directory, under a
#: name that is the sha256 of the checkout's ABSOLUTE path — which is each side's own
#: directory, so the two notes are two filenames. What is IN them is the whole of what either
#: implementation decided, and `_unrecorded_facts` compares that.
UNRECORDED_NOTE = {
    ".charter/unrecorded": "keyed by sha256 of the checkout's own absolute path, which names "
                           "each side's directory; the note's CONTENT is compared through "
                           "`facts`",
}


def _unrecorded_facts(root: Path) -> str:
    """The checkout's own facts, plus every failed-publish note charter kept, by content.

    **A missing note is a failure of the scenario**: without a note neither side decided
    anything, and comparing "nothing" against "nothing" reports ok for an implementation that
    never reached the rule.
    """
    notes = sorted((root / ".charter" / "unrecorded").glob("*.json"))
    if not notes:
        raise SystemExit(
            "setup: no record-publish failure was kept, so this scenario would compare "
            "nothing about `unrecorded` and report ok"
        )
    kept = "\n".join(note.read_text() for note in notes)
    return f"{_checkout_facts(root)}\nunrecorded:\n{kept}"


def _a_checkout_whose_root_refuses_its_record(root: Path) -> None:
    """The checkout's ROOT refuses a write while `.git/info` still takes one — charter's
    ruling H, in the one shape that reaches it.

    The exclude block lands and the record cannot, so charter writes NOTHING: a file on disk
    whose record the next launch cannot vouch for is a file whose exclude line that launch
    drops, into somebody else's repository.
    """
    _a_checkout_inside_a_workspace(root)
    os.chmod(root / "workspaces" / "beta" / "svc", 0o555)


def _checkout_facts(root: Path) -> str:
    """charter's block in the checkout's `info/exclude`, and its `git status`.

    The second half is the guarantee and not decoration: charter is a guest in that
    repository, and a layer that showed up as untracked noise in the operator's `git status`
    is the failure the block exists to prevent. Both sides must agree about both.

    **A missing checkout is a failure of the scenario, not an answer.** Without this, a setup
    that silently made nothing would compare `<none>` against `<none>` and report `ok` for any
    implementation at all — the exact shape this suite has been caught by twice.
    """
    return _guest_facts(root, ["svc"])


def _guest_facts(root: Path, rels: list[str]) -> str:
    """[`_checkout_facts`] for several guest trees at once — a clone, a worktree beside it,
    and a piece under `.worktrees/`.

    Each one's `git status` is compared, because the block is SHARED: a line the clone's wire
    adds hides that path in every tree reading it, and a line one of them drops shows
    charter's file in another's `git status`. Comparing only the clone would miss exactly the
    failure `_shared_rels` exists to prevent.
    """
    out = []
    for rel in rels:
        tree = root.joinpath("workspaces", "beta", *rel.split("/"))
        if not (tree / ".git").exists():
            raise SystemExit(
                f"setup: there is no checkout at workspaces/beta/{rel}, so this scenario "
                "would compare nothing about it and report ok"
            )
        exclude = subprocess.run(
            ["git", "-C", str(tree), "rev-parse", "--path-format=absolute", "--git-common-dir"],
            capture_output=True, text=True,
            env={"PATH": "/usr/bin:/bin", "HOME": "/nonexistent", "GIT_CONFIG_NOSYSTEM": "1"},
        ).stdout.strip()
        block = Path(exclude) / "info" / "exclude" if exclude else None
        status = subprocess.run(
            ["git", "-C", str(tree), "status", "--porcelain"], capture_output=True, text=True,
            env={"PATH": "/usr/bin:/bin", "HOME": "/nonexistent", "GIT_CONFIG_NOSYSTEM": "1"},
        ).stdout
        text = block.read_text() if block and block.exists() else "<none>"
        out.append(f"--- {rel}\nexclude:\n{text}\nstatus:\n{status}")
    return "\n".join(out)


def _clone_and_worktree_facts(root: Path) -> str:
    return _guest_facts(root, ["svc", "svc-wt"])


def _piece_facts(root: Path) -> str:
    return _guest_facts(root, ["svc", ".worktrees/svc/p1"])


def _a_clone_in_alpha_on_a_branch_nothing_snapshotted(root: Path) -> None:
    """A real clone in `alpha`, on `main`, that `alpha`'s committed manifest does not name.

    What `fork`'s union is for (charter#81): a workspace with clones and no snapshot of them
    inherited zero. It is also the only thing that drives the git verb per repo — a
    `rev-parse` through the hardened runner — since a manifest row needs no git at all.
    """
    tree = root / "workspaces" / "alpha" / "svc2"
    tree.mkdir(parents=True)
    _git(root.parent, "init", "-q", "-b", "main", ".", cwd=tree)
    (tree / "README.md").write_text("# svc2\n")
    _git(root.parent, "add", "-A", cwd=tree)
    _git(root.parent, "commit", "-q", "-m", "one", cwd=tree)


#: `fork` writes nothing into `alpha`, so there is no `facts` to compare instead — what this
#: clone is here for is entirely the branch name that reaches the fork's manifest, which the
#: tree comparison reads out of `workspaces/gamma/workspace.json`.
ALPHA_CLONE_GIT = {
    "workspaces/alpha/svc2/.git": "read by `fork`, never written; the index carries inodes "
                                  "and mtimes no two runs share"
}


# charter workspace restore — and the trap in testing a CREDENTIALED pull (M2.26)
# --------------------------------------------------------------------------------------------
#
# `restore` checks out each recorded branch and then pulls it over the forge's own credential.
# Making that pull observable takes TWO remotes, and the reason is the same one `_forge_trap`
# above exists for, one level down.
#
# `gitpolicy.forge_for` reads `git remote get-url origin`, and `get-url` APPLIES
# `url.<x>.insteadOf` (measured, git 2.50.1: `git config remote.origin.url` gives the URL in the
# file, `get-url` gives the rewritten one). So the rewrite that sends a fetch to a bare
# repository beside the plane also makes `origin` a host charter cannot place — and both
# implementations answer "origin host isn't a known/declared forge — skipped" over a pull
# NEITHER performed. Green, empty diff, nothing tested.
#
# A branch may track a remote that is not `origin`, which is an ordinary git configuration and
# splits the two questions apart: `origin` decides which forge's credential the pull is given,
# `branch.<name>.remote` decides where it fetches from. So `origin` stays on a host charter
# knows, the upstream is a local bare, and the pulled commit lands in the working tree where the
# tree comparison reads it — which is what a mutation that drops the pull loses.


def _a_clone_whose_upstream_is_a_local_bare(root: Path) -> None:
    """`alpha/api`: a clone one commit behind a bare repository beside the plane.

    `origin` is the SSH form on a host charter knows, untouched by any rewrite, so
    `gitpolicy.forge_for` places it and the pull is REACHED with that forge's credential
    helper. The branch's upstream is the local bare, so the pull transfers and `EXTRA.md`
    appears in the tree.

    The manifest is written here rather than added to the fixture's, so this scenario's rows
    are exactly the ones under test: one repo, pinned to the branch that is behind.
    """
    side = root.parent
    _identity(side, [])
    src = side / "forge" / "api-src"
    src.mkdir(parents=True)
    _git(side, "init", "-q", "-b", "main", ".", cwd=src)
    (src / "README.md").write_text("# api\n")
    _git(side, "add", "-A", cwd=src)
    _git(side, "commit", "-q", "-m", "one", cwd=src)
    bare = side / "forge" / "api.git"
    _git(side, "clone", "-q", "--bare", str(src), str(bare))

    work = root / "workspaces" / "alpha" / "api"
    _git(side, "clone", "-q", str(bare), str(work))
    _git(side, "remote", "set-url", "origin", "git@github.com:acme/api.git", cwd=work)
    _git(side, "remote", "add", "upstream", f"file://{bare}", cwd=work)
    _git(side, "config", "branch.main.remote", "upstream", cwd=work)
    _git(side, "config", "branch.main.merge", "refs/heads/main", cwd=work)

    # One commit the clone does not have yet. Its author and committer dates come from
    # `FORGE_ENV`, so the commit charter pulls has the same sha on both sides.
    (src / "EXTRA.md").write_text("pulled\n")
    _git(side, "add", "-A", cwd=src)
    _git(side, "commit", "-q", "-m", "two", cwd=src)
    _git(side, "push", "-q", str(bare), "main", cwd=src)

    (root / "workspaces" / "alpha" / "workspace.json").write_text(json.dumps({
        "name": "alpha", "description": "", "repos": [{"name": "api", "branch": "main"}],
        "updated_at": "2026-03-02T09:24:00+00:00", "updated_by": "Fixture User",
    }, indent=2) + "\n")


def _a_manifest_naming_a_path_and_a_branch_that_is_an_option(root: Path) -> None:
    """The two guards charter#334 closed, in one manifest: a repo NAME that is a path, and a
    BRANCH that `git checkout` would read as an option.

    `api` is a real clone with an origin charter can place, so the dash guard is reached —
    it sits after the forge lookup, and a row that never got that far would prove nothing
    about it. The `../esc` row is refused before any path is joined.
    """
    _a_clone_whose_upstream_is_a_local_bare(root)
    (root / "workspaces" / "alpha" / "workspace.json").write_text(json.dumps({
        "name": "alpha", "description": "",
        "repos": [{"name": "../esc", "branch": "main"}, {"name": "api", "branch": "-b"}],
        "updated_at": "2026-03-02T09:24:00+00:00", "updated_by": "Fixture User",
    }, indent=2) + "\n")


def _a_manifest_row_with_no_branch(root: Path) -> None:
    """charter#884's row: membership with no branch, which `restore` treats as restored by
    existing — and answers BEFORE the forge lookup, because there is nothing to check out."""
    _a_clone_whose_upstream_is_a_local_bare(root)
    (root / "workspaces" / "alpha" / "workspace.json").write_text(json.dumps({
        "name": "alpha", "description": "", "repos": [{"name": "api"}],
        "updated_at": "2026-03-02T09:24:00+00:00", "updated_by": "Fixture User",
    }, indent=2) + "\n")


#: The restored clone's `.git`: an index with inodes and mtimes no two runs share, and a
#: `remote.upstream.url` naming each side's own directory. What matters in it is compared
#: through `facts`.
RESTORED_CLONE_GIT = {
    "workspaces/alpha/api/.git": "compared through `facts`: the index carries inodes and "
                                 "mtimes, and the upstream URL names each side's own directory"
}


def _restored_clone_facts(root: Path) -> str:
    """Which commit the clone is on, which branch, and whether anything is left showing.

    **A missing clone is a failure of the scenario, not an answer.** Without this a setup that
    silently made nothing would compare `<none>` against `<none>` and report `ok` for any
    implementation at all.
    """
    tree = root / "workspaces" / "alpha" / "api"
    if not (tree / ".git").is_dir():
        raise SystemExit(
            "setup: there is no clone at workspaces/alpha/api, so this scenario would compare "
            "nothing about a restore and report ok"
        )
    env = {"PATH": "/usr/bin:/bin", "HOME": "/nonexistent", "GIT_CONFIG_NOSYSTEM": "1"}

    def git(*args: str) -> str:
        done = subprocess.run(["git", "-C", str(tree), *args], capture_output=True, text=True,
                              env=env)
        return done.stdout.strip() if done.returncode == 0 else f"<rc {done.returncode}>"

    return (f"head: {git('rev-parse', 'HEAD')}\n"
            f"branch: {git('rev-parse', '--abbrev-ref', 'HEAD')}\n"
            f"status:\n{git('status', '--porcelain')}\n")


M28_SCENARIOS = [
    # ---- charter handoff: every refusal in front of the open ----------------------------
    Scenario(
        name="handoff-refuses-a-name-that-cannot-be-a-workspace",
        plane="daily",
        python=["handoff", "../../esc"],
        pins_the_clock=False,
        stdin=A_BRIEF,
        refusal="cannot name a workspace",
        same_stderr=True,
    ),
    Scenario(
        name="handoff-refuses-vision-without-create",
        plane="daily",
        python=["handoff", "alpha", "--vision", "ship it"],
        pins_the_clock=False,
        stdin=A_BRIEF,
        refusal="--vision describes a workspace this call creates",
        same_stderr=True,
    ),
    Scenario(
        name="handoff-refuses-create-without-vision",
        plane="daily",
        python=["handoff", "brand-new", "--create"],
        pins_the_clock=False,
        stdin=A_BRIEF,
        refusal="--create needs --vision",
        same_stderr=True,
    ),
    Scenario(
        name="handoff-refuses-create-over-a-workspace-that-is-already-there",
        plane="daily",
        python=["handoff", "alpha", "--create", "--vision", "ship it"],
        pins_the_clock=False,
        stdin=A_BRIEF,
        refusal="already exists, and --create only makes a new one",
        same_stderr=True,
    ),
    Scenario(
        name="handoff-refuses-a-workspace-this-plane-does-not-have",
        plane="daily",
        python=["handoff", "nowhere"],
        pins_the_clock=False,
        stdin=A_BRIEF,
        refusal="no workspace 'nowhere' on this plane",
        same_stderr=True,
    ),
    Scenario(
        name="handoff-refuses-a-persona-this-plane-does-not-define",
        plane="daily",
        python=["handoff", "alpha", "--persona", "ghost"],
        pins_the_clock=False,
        stdin=A_BRIEF,
        refusal="no persona 'ghost'",
        same_stderr=True,
    ),
    Scenario(
        name="handoff-refuses-an-empty-brief",
        plane="daily",
        python=["handoff", "alpha"],
        pins_the_clock=False,
        stdin="   \n\t\n",
        refusal="the brief on stdin is empty",
        same_stderr=True,
    ),
    Scenario(
        name="handoff-refuses-a-credential-shaped-brief-by-kind",
        plane="daily",
        python=["handoff", "alpha"],
        pins_the_clock=False,
        stdin=A_BRIEF_WITH_A_SECRET,
        refusal="the brief looks like it carries a secret",
        same_stderr=True,
        # The whole point of naming a KIND. `same_stderr` cannot stand in for this: two
        # implementations that both quoted the credential would match each other exactly.
        never_says=(THE_VALUE_IN_THAT_BRIEF,),
    ),
    Scenario(
        name="handoff-with-nothing-to-open-a-chat-in-prints-the-command-and-writes-nothing",
        plane="daily",
        python=["handoff", "alpha"],
        pins_the_clock=False,
        stdin=A_BRIEF,
        refusal="Run this in a new terminal instead:",
        stderr_differs=HANDOFF_HAS_NO_FRAME,
    ),
    # ---- charter workspace live ---------------------------------------------------------
    Scenario(
        name="workspace-live-shares-a-workspaces-manifest-and-memory",
        plane="daily",
        python=["workspace", "live", "beta"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="workspace-live-off-makes-it-private-again",
        plane="daily",
        python=["workspace", "live", "alpha", "--off"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="workspace-live-on-a-workspace-that-is-already-live-changes-nothing",
        plane="daily",
        python=["workspace", "live", "alpha"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="workspace-live-refuses-a-workspace-this-plane-does-not-have",
        plane="daily",
        python=["workspace", "live", "nowhere"],
        pins_the_clock=False,
        refusal="no workspace 'nowhere'",
        same_stderr=True,
    ),
    # ---- charter workspace remove -------------------------------------------------------
    Scenario(
        name="workspace-remove-takes-the-workspace-and-its-clones",
        plane="daily",
        python=["workspace", "remove", "beta"],
        pins_the_clock=False,
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        same_stderr=True,
    ),
    Scenario(
        name="workspace-remove-reports-the-open-todos-it-discards-and-removes-anyway",
        plane="daily",
        python=["workspace", "remove", "alpha"],
        pins_the_clock=False,
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        same_stderr=True,
    ),
    Scenario(
        name="workspace-remove-refuses-a-clone-it-could-not-read",
        plane="daily",
        setup=_a_clone_charter_cannot_read,
        python=["workspace", "remove", "alpha"],
        pins_the_clock=False,
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        refusal="Refusing to remove 'alpha' — this would discard work:",
        stderr_differs=REMOVE_QUOTES_GIT,
    ),
    Scenario(
        name="workspace-remove-refuses-a-workspace-this-plane-does-not-have",
        plane="daily",
        python=["workspace", "remove", "nowhere"],
        pins_the_clock=False,
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        refusal="no workspace 'nowhere'",
        same_stderr=True,
    ),
    # ---- charter workspace use / unlock / default ----------------------------------------
    Scenario(
        name="workspace-use-writes-the-session-pointer-and-takes-the-lock",
        plane="daily",
        python=["workspace", "use", "beta"],
        pins_the_clock=False,
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        ignore={
            f".charter/persona-state/trace/{FRESH_SESSION}.jsonl": (
                "charter records every selection in its trace store; this binary writes no "
                "trace at all, which is a whole store and not this command's to port"
            )
        },
        same_stderr=True,
    ),
    Scenario(
        name="workspace-use-refuses-a-name-no-workspace-has",
        plane="daily",
        python=["workspace", "use", "nowhere"],
        pins_the_clock=False,
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        refusal="no workspace named 'nowhere'",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-unlock-releases-this-sessions-lock",
        plane="daily",
        setup=_a_session_lock,
        python=["workspace", "unlock"],
        pins_the_clock=False,
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        same_stderr=True,
    ),
    Scenario(
        name="workspace-unlock-with-no-lock-says-there-was-nothing-to-unlock",
        plane="daily",
        python=["workspace", "unlock"],
        pins_the_clock=False,
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        same_stderr=True,
    ),
    Scenario(
        name="workspace-default-nominates-a-workspace",
        plane="daily",
        python=["workspace", "default", "beta"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="workspace-default-with-no-name-says-none-is-declared",
        plane="daily",
        python=["workspace", "default"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="workspace-default-clear-with-nothing-declared-removes-nothing",
        plane="daily",
        python=["workspace", "default", "--clear"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="workspace-default-refuses-a-value-that-is-not-a-workspace-name",
        plane="daily",
        python=["workspace", "default", "../../esc"],
        pins_the_clock=False,
        refusal="is not a workspace name",
        same_stderr=True,
    ),
    # ---- charter workspace create ---------------------------------------------------------
    Scenario(
        name="workspace-create-makes-the-baseline-and-wires-the-planes-layer-into-it",
        plane="daily",
        python=["workspace", "create", "gamma"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-create-records-the-vision-it-was-given",
        plane="daily",
        python=["workspace", "create", "gamma", "--vision", "Ship the importer"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-create-live-shares-the-new-workspace-from-birth",
        plane="daily",
        python=["workspace", "create", "gamma", "--live"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-create-on-a-workspace-that-is-already-there-changes-nothing",
        plane="daily",
        python=["workspace", "create", "alpha"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-create-refuses-a-name-that-is-not-a-workspace-name",
        plane="daily",
        python=["workspace", "create", "../esc"],
        refusal="invalid workspace name",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-create-use-in-a-locked-session-creates-it-and-refuses-the-selection",
        plane="daily",
        python=["workspace", "create", "gamma", "--use"],
        # Exit 2: the workspace WAS created and only the selection was refused, which a script
        # has to be able to tell from a name that is not a workspace.
        refusal="locked to 'alpha' for this session",
        same_stderr=True,
        ignore={
            ".charter/persona-state/trace/fixture-session-1.jsonl": (
                "charter records the refused selection in its trace store; this binary writes "
                "no trace at all, which is a whole store and not this command's to port"
            )
        },
    ),
    Scenario(
        name="workspace-create-use-selects-the-workspace-it-just-made",
        plane="daily",
        python=["workspace", "create", "gamma", "--use"],
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        ignore={
            f".charter/persona-state/trace/{FRESH_SESSION}.jsonl": (
                "charter records every selection in its trace store; this binary writes no "
                "trace at all, which is a whole store and not this command's to port"
            )
        },
        same_stderr=True,
    ),
    Scenario(
        name="workspace-use-create-scaffolds-rather-than-leaving-a-bare-directory",
        plane="daily",
        python=["workspace", "use", "gamma", "--create"],
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        ignore={
            f".charter/persona-state/trace/{FRESH_SESSION}.jsonl": (
                "charter records every selection in its trace store; this binary writes no "
                "trace at all, which is a whole store and not this command's to port"
            )
        },
        same_stderr=True,
    ),
    # ---- charter workspace reinit ---------------------------------------------------------
    Scenario(
        name="workspace-reinit-on-a-current-workspace-says-there-is-nothing-to-do",
        plane="daily",
        python=["workspace", "reinit", "alpha"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-reinit-refuses-a-workspace-this-plane-does-not-have",
        plane="daily",
        python=["workspace", "reinit", "nowhere"],
        refusal="no workspace 'nowhere'",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-reinit-all-on-a-current-plane-says-there-is-nothing-to-do",
        plane="daily",
        python=["workspace", "reinit", "--all"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-reinit-adds-a-baseline-file-an-older-charter-never-made",
        plane="daily",
        setup=_a_workspace_missing_a_baseline_file,
        python=["workspace", "reinit", "beta"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-reinit-bumps-a-structure-stamp-an-older-charter-left",
        plane="daily",
        setup=_a_workspace_an_older_charter_stamped,
        python=["workspace", "reinit", "beta"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-reinit-refreshes-a-layer-the-plane-has-moved-past",
        plane="daily",
        setup=_a_workspace_layer_the_plane_has_moved_past,
        python=["workspace", "reinit", "beta"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-reinit-leaves-a-settings-file-charter-did-not-write-and-says-so",
        plane="daily",
        setup=_a_settings_file_the_operator_wrote,
        python=["workspace", "reinit", "beta"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-reinit-with-no-name-repairs-the-workspace-the-ladder-resolves",
        plane="daily",
        setup=_a_workspace_missing_a_baseline_file,
        python=["workspace", "reinit"],
        # No name and no `--all`: the session pointer names `alpha`, so `beta`'s missing file
        # is NOT the one repaired. A ladder that resolved differently would repair the wrong
        # workspace and say so in the line below it.
        same_stderr=True,
    ),
    Scenario(
        name="workspace-use-brings-an-existing-workspace-up-to-the-layout-on-the-way-in",
        plane="daily",
        setup=_a_workspace_layer_the_plane_has_moved_past,
        python=["workspace", "use", "beta"],
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        ignore={
            f".charter/persona-state/trace/{FRESH_SESSION}.jsonl": (
                "charter records every selection in its trace store; this binary writes no "
                "trace at all, which is a whole store and not this command's to port"
            )
        },
        # `use` runs `ensure`, so selecting a workspace is also where one an older charter
        # left picks up what this charter writes. It says nothing about the repair — the
        # resulting plane is the whole of the comparison here.
        same_stderr=True,
    ),
    Scenario(
        name="workspace-reinit-withdraws-a-file-the-plane-no-longer-declares",
        plane="daily",
        setup=_a_generated_file_the_plane_stopped_declaring,
        python=["workspace", "reinit", "--all"],
        same_stderr=True,
    ),
    # ---- charter workspace reinit: the CHECKOUT inside a workspace (M2.24, M2.25) --------
    #
    # The scenario M2.22 said it could not write. Both sides must write the same layer into
    # somebody else's repository, hide it in the same `info/exclude` block, and leave that
    # repository's own `git status` empty.
    Scenario(
        name="workspace-reinit-wires-a-checkout-inside-the-workspace",
        plane="daily",
        setup=_a_checkout_inside_a_workspace,
        python=["workspace", "reinit", "beta"],
        facts=_checkout_facts,
        ignore=CHECKOUT_GIT,
        same_stderr=True,
    ),
    Scenario(
        # The same wire reached through `--all`, which is what an operator runs after an
        # upgrade — and the closing line's arithmetic over a workspace whose repairs are all
        # inside a checkout.
        name="workspace-reinit-all-wires-a-checkout-and-counts-its-rows-as-repairs",
        plane="daily",
        setup=_a_checkout_inside_a_workspace,
        python=["workspace", "reinit", "--all"],
        facts=_checkout_facts,
        ignore=CHECKOUT_GIT,
        same_stderr=True,
    ),
    Scenario(
        # The ownership rule, in the one place it costs something: a checkout is somebody
        # else's repository, so the file is left exactly as it is, NO block is written — a
        # line for that path would hide their own untracked file from their own `git status`
        # — and the advice is `git add -f`, never "move it aside".
        name="workspace-reinit-leaves-a-settings-file-of-yours-in-a-checkout-and-says-how-to-commit-it",
        plane="daily",
        setup=_a_checkout_holding_a_settings_file_of_yours,
        python=["workspace", "reinit", "beta"],
        facts=_checkout_facts,
        ignore=CHECKOUT_GIT,
        same_stderr=True,
    ),
    Scenario(
        # `use` runs `ensure`, and `ensure` scaffolds — so selecting a workspace is also where
        # a checkout an older charter left picks up the layer. It says nothing about the
        # repair, so the resulting plane IS the whole of the comparison here.
        name="workspace-use-wires-a-checkout-inside-the-workspace-on-the-way-in",
        plane="daily",
        setup=_a_checkout_inside_a_workspace,
        python=["workspace", "use", "beta"],
        env={"CHARTER_SESSION_ID": FRESH_SESSION},
        facts=_checkout_facts,
        ignore={
            **CHECKOUT_GIT,
            f".charter/persona-state/trace/{FRESH_SESSION}.jsonl": (
                "charter records every selection in its trace store; this binary writes no "
                "trace at all, which is a whole store and not this command's to port"
            ),
        },
        same_stderr=True,
    ),
    # ---- charter workspace fork ------------------------------------------------------------
    Scenario(
        name="workspace-fork-carries-the-charter-the-memory-and-the-open-todos",
        plane="daily",
        python=["workspace", "fork", "alpha", "gamma"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-fork-of-a-workspace-with-nothing-to-inherit",
        plane="daily",
        python=["workspace", "fork", "beta", "gamma"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-fork-live-shares-the-fork-from-birth",
        plane="daily",
        python=["workspace", "fork", "alpha", "gamma", "--live"],
        same_stderr=True,
    ),
    Scenario(
        # charter#81's union, and the git verb per repo the whole port is here for: `svc2` is
        # a clone `alpha`'s manifest does not name, so its branch can only come from a
        # `rev-parse` — and the fork has to say that the branch it inherited was never
        # snapshotted.
        name="workspace-fork-inherits-a-clone-the-manifest-never-recorded-and-says-where-its-branch-came-from",
        plane="daily",
        setup=_a_clone_in_alpha_on_a_branch_nothing_snapshotted,
        python=["workspace", "fork", "alpha", "gamma"],
        ignore=ALPHA_CLONE_GIT,
        same_stderr=True,
    ),
    Scenario(
        name="workspace-duplicate-is-the-same-verb-as-fork",
        plane="daily",
        python=["workspace", "duplicate", "beta", "gamma"],
        same_stderr=True,
    ),
    Scenario(
        name="workspace-fork-refuses-a-name-that-is-not-a-workspace-name",
        plane="daily",
        python=["workspace", "fork", "alpha", "../esc"],
        refusal="invalid workspace name '../esc'",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-fork-refuses-a-workspace-forked-onto-its-own-name",
        plane="daily",
        python=["workspace", "fork", "alpha", "alpha"],
        refusal="source and fork names are the same",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-fork-refuses-a-source-this-plane-does-not-have",
        plane="daily",
        python=["workspace", "fork", "nowhere", "gamma"],
        refusal="no workspace 'nowhere'",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-fork-refuses-a-fork-name-that-is-already-a-workspace",
        plane="daily",
        python=["workspace", "fork", "alpha", "beta"],
        refusal="already exists — pick another name or remove it first",
        same_stderr=True,
    ),
    # ---- the four row states only a CHECKOUT can reach (M2.26) ---------------------------
    #
    # All four rest on one piece of bookkeeping: `git worktree list` per repository, and then
    # `git status` per path per other tree before a line is added or left out. Each scenario
    # below makes a real repository in its own setup, because a fixture plane cannot carry
    # one — git will not track a path inside a `.git` directory, so `generate.py` prunes every
    # checkout.
    Scenario(
        # The shared block, with nothing of anybody's in the way: a clone and a linked
        # worktree of it read ONE `info/exclude`, and what each wire writes into it has to
        # hold what BOTH of them need. A second tree rewriting the block to its own list
        # alone is how charter's own history dropped the line for `.claude/settings.json`
        # into somebody else's repository.
        name="workspace-reinit-wires-a-clone-and-a-worktree-of-it-through-one-shared-block",
        plane="daily",
        setup=_a_clone_and_a_worktree_of_it,
        python=["workspace", "reinit", "beta"],
        facts=_clone_and_worktree_facts,
        ignore=SIBLING_GIT,
        same_stderr=True,
    ),
    Scenario(
        # `unhidden`, charter#1072. The line for charter's own `.claude/settings.json` is
        # left OUT, because it would hide the operator's untracked file at that path in the
        # sibling. charter's file is still written, and shows in this checkout's own `git
        # status` — which `facts` compares, so a port that hid it anyway is red.
        name="workspace-reinit-leaves-a-line-out-rather-than-hide-your-file-in-a-sibling-worktree",
        plane="daily",
        setup=_a_worktree_of_it_holding_a_settings_file_of_yours,
        python=["workspace", "reinit", "beta"],
        facts=_clone_and_worktree_facts,
        ignore=SIBLING_GIT,
        stderr_mask=PLANE_COPY_MASK,
        same_stderr=True,
    ),
    Scenario(
        # `withheld`, the other half of charter#1072: a MACHINE-LOCAL file charter cannot
        # hide is one `git add` from being committed into somebody else's repository, so it
        # is not written at all — while the shared settings and the mirrored agents still
        # are, and the plane's committed rules still reach the checkout.
        name="workspace-reinit-withholds-a-machine-local-file-it-cannot-hide-and-writes-the-rest",
        plane="daily",
        setup=_a_worktree_of_it_holding_your_machine_local_file,
        python=["workspace", "reinit", "beta"],
        facts=_clone_and_worktree_facts,
        ignore=SIBLING_GIT,
        stderr_mask=PLANE_COPY_MASK,
        same_stderr=True,
    ),
    Scenario(
        # The half the four rows came with, and the one an operator feels: a PIECE at
        # `.worktrees/<repo>/<piece>` is where `charter wt add` tells a worker to start a
        # session, and nothing that walks a workspace's children reaches it.
        name="workspace-reinit-reaches-a-piece-under-the-workspaces-worktrees",
        plane="daily",
        setup=_a_piece_under_the_workspaces_worktrees,
        python=["workspace", "reinit", "beta"],
        facts=_piece_facts,
        ignore=PIECE_GIT,
        same_stderr=True,
    ),
    Scenario(
        # `unlisted`. Measured on git 2.50.1, a git that cannot read `worktrees/` lists the
        # clone alone and exits 0 — so the worktree it leaves out gets no layer, and passed
        # over in silence `reinit` would say "nothing to do" over it.
        name="workspace-reinit-names-a-repository-whose-worktrees-git-could-not-list",
        plane="daily",
        setup=_a_repository_whose_worktrees_git_cannot_list,
        python=["workspace", "reinit", "beta"],
        facts=_checkout_facts,
        ignore=SIBLING_GIT,
        stderr_mask=PLANE_COPY_MASK,
        same_stderr=True,
    ),
    Scenario(
        # `unrecorded`, charter's ruling H: the block lands, the record cannot be published,
        # and NOTHING is written — because a file whose record the next launch cannot vouch
        # for is a file whose exclude line that launch drops.
        name="workspace-reinit-writes-nothing-where-it-could-not-record-what-it-would-write",
        plane="daily",
        setup=_a_checkout_whose_root_refuses_its_record,
        python=["workspace", "reinit", "beta"],
        facts=_unrecorded_facts,
        ignore={**CHECKOUT_GIT, **UNRECORDED_NOTE},
        same_stderr=True,
    ),
    # ---- charter workspace restore, and `fork --restore` (M2.26) --------------------------
    Scenario(
        # The one scenario that reaches the CREDENTIALED PULL and sees it do something: the
        # clone is one commit behind, and `EXTRA.md` is in the tree afterwards on both sides.
        # See the note over `_a_clone_whose_upstream_is_a_local_bare` for why that takes two
        # remotes.
        name="workspace-restore-checks-out-the-recorded-branch-and-pulls-it",
        plane="daily",
        setup=_a_clone_whose_upstream_is_a_local_bare,
        python=["workspace", "restore", "alpha"],
        facts=_restored_clone_facts,
        ignore=RESTORED_CLONE_GIT,
        same_stderr=True,
    ),
    Scenario(
        # charter#325/#334/#328 and #334's second half, in one manifest. The dash guard sits
        # AFTER the forge lookup, so the row it refuses is a real clone on a host charter can
        # place — a row that never got that far would prove nothing about it.
        name="workspace-restore-refuses-a-manifest-name-that-is-a-path-and-a-branch-that-is-an-option",
        plane="daily",
        setup=_a_manifest_naming_a_path_and_a_branch_that_is_an_option,
        python=["workspace", "restore", "alpha"],
        facts=_restored_clone_facts,
        ignore=RESTORED_CLONE_GIT,
        same_stderr=True,
    ),
    Scenario(
        # charter#884: membership with no branch is restored by existing, and is answered
        # BEFORE the forge lookup — there is nothing to check out and nothing to pull.
        name="workspace-restore-treats-a-row-with-no-branch-as-restored-by-existing",
        plane="daily",
        setup=_a_manifest_row_with_no_branch,
        python=["workspace", "restore", "alpha"],
        facts=_restored_clone_facts,
        ignore=RESTORED_CLONE_GIT,
        same_stderr=True,
    ),
    Scenario(
        name="workspace-restore-refuses-a-workspace-whose-manifest-records-no-repos",
        plane="daily",
        python=["workspace", "restore", "beta"],
        refusal="no manifest for 'beta'",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-restore-on-demand-lists-every-repo-and-clones-none",
        plane="daily",
        python=["workspace", "restore", "alpha", "--on-demand"],
        same_stderr=True,
    ),
    Scenario(
        # The plane has no inventory, so the clone half has nothing to clone FROM — and the
        # rows are then skipped one by one rather than reported as restored.
        name="workspace-restore-with-nothing-to-clone-from-skips-every-repo-it-could-not-clone",
        plane="daily",
        python=["workspace", "restore", "alpha"],
        same_stderr=True,
    ),
    Scenario(
        # M2.24 took `--restore` and then said what it had not done. It restores now, and the
        # line that used to be the gap is the restore's own report.
        name="workspace-fork-with-restore-clones-the-inherited-repos",
        plane="daily",
        python=["workspace", "fork", "alpha", "gamma", "--restore"],
        same_stderr=True,
    ),
    # ---- charter persona default ---------------------------------------------------------
    Scenario(
        name="persona-default-declares-the-front-door-in-charter-toml",
        plane="daily",
        python=["persona", "default", "devops"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="persona-default-with-no-name-says-none-is-declared",
        plane="daily",
        python=["persona", "default"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="persona-default-clear-with-nothing-declared-rewrites-nothing",
        plane="daily",
        python=["persona", "default", "--clear"],
        pins_the_clock=False,
        same_stderr=True,
    ),
    Scenario(
        name="persona-default-refuses-a-persona-this-plane-does-not-define",
        plane="daily",
        python=["persona", "default", "ghost"],
        pins_the_clock=False,
        refusal="no persona 'ghost'",
        same_stderr=True,
    ),
]


# M2.21: charter's own documentation, printed by the install that implements it
# --------------------------------------------------------------------------------------------
#
# `docs list` and `docs show` were clap usage errors. They are ported rather than refused,
# which means charter-app carries a SECOND copy of charter's `docs/*.md` — vendored from the
# wheel's `charter/_docs` at the oracle's pinned commit and compiled in by `build.rs`, exactly
# as `news/` is, and for the reason `charter/docsrc.py` gives: the page a user reads must come
# from the same install as the behaviour.
#
# Two copies need a tie, and it is the same pair `news` uses: one generated scenario per topic
# renders `docs show <topic>` on both sides byte for byte, and `check_docs_corpus` compares the
# two corpora as digests before any of them run — because a generated scenario cannot see a
# page DELETED from the directory it was generated from.

DOCS_DIR = REPO / "crates" / "charter-core" / "docs"

#: Ask the ORACLE what it serves, as a subprocess under the interpreter the scenarios use.
#: `docsrc.topics()` and not a glob of the directory: what is compared is what `docs show`
#: would print, and the topic list is the thing that decides that.
_DOCS_DIGEST = (
    "import hashlib\n"
    "from charter import docsrc\n"
    "root = docsrc.source()\n"
    "for topic in (docsrc.topics() if root else []):\n"
    "    print(topic, hashlib.sha256((root / (topic + '.md')).read_bytes()).hexdigest())\n"
)

#: The one thing in `docs list` that cannot match: Python names the DIRECTORY it read the
#: pages out of, and charter-app names the binary they are compiled into. The sentence around
#: it — and every topic in the listing — still has to match byte for byte. A lookbehind, so
#: `charter documentation (` and `):` are themselves compared rather than masked away.
DOCS_SOURCE_MASK = [
    (r"(?<=charter documentation \()[^)]*(?=\):)",
     "the pages' source is a path into the install carrying them, and the two installs are "
     "different things: a wheel's charter/_docs, and this binary"),
]


@dataclass(frozen=True)
class PageDiverges:
    """A vendored page charter-app deliberately holds a DIFFERENT version of.

    `Divergence` for the docs corpus rather than for a command, and the same refusal to be a
    waiver (charter-app#119). `check_docs_corpus` holds the two copies of every page byte for
    byte, which is exactly what it is for; a page describing behaviour ADR 0035 reversed
    cannot pass that and cannot be corrected where it lives, because spec decision 17 freezes
    the Python charter and a frozen oracle documenting behaviour it does not have would be
    worse than the drift.

    Skipping the page would have been the cheap answer and the wrong one: a skipped page is
    one nothing compares, so the NEXT drift in it — anywhere in it, for any reason — lands in
    silence. So this does not skip. It states what charter's copy says and what charter-app's
    says instead, the run rewrites the oracle's page with those pairs, and the result is
    compared byte for byte. Everything the divergence does not name is held to the bar it
    always was.

    It fails loudly in both directions, which is the point:

    - **`why` must cite the record** — an ADR and a spec decision — because in six months the
      only thing between this and two copies nobody can explain is a sentence with a number
      in it.
    - **charter must still say its half.** A `theirs` that has left the oracle's page is a
      divergence FROM something that is gone: the direction a reviewer forgets, and the one
      that rots first.
    - **charter-app must say its half**, so a declaration cannot outlive the edit it records.
    - **the two copies must still differ.** If the pages ever match again the divergence is
      over, and the run says so instead of passing quietly.

    `docs show` prints the page's bytes, so the same pairs drive the per-topic scenario's
    stdout through `Scenario.stdout_rewrite`. One table, both comparisons — the alternative is
    a corpus check and a render check that can disagree about which page is the right one.
    """

    #: Prose naming the records that decided it. Must cite an ADR and a spec decision.
    why: str
    #: `(charter's words, charter-app's words)`, applied in order, once each.
    rewrites: tuple[tuple[str, str], ...]


#: Why both diverged pages diverge. One string, because it is one decision and a second copy
#: of it would be a second thing to keep true.
DOCS_DIVERGE_WHY = (
    "ADR 0035 reversed `charter init`'s default at the top of an existing git repository: the "
    "plane goes in a directory of its own and the repo becomes its first clone, with "
    "`--plane-is-this-repo` as the opt-in. charter-app spec decision 27 carries it, and "
    "`INIT_IN_A_REPO_DIVERGES` is the same decision holding the BEHAVIOUR apart — this is its "
    "documentation. Each page says in its own text which implementation it describes, which is "
    "the condition charter-app#119 puts on holding a different copy at all."
)

#: The pages charter-app holds its own version of, and exactly what differs in each.
#:
#: Read `PageDiverges` before changing anything here, and ADR 0035 before deciding it should
#: not exist. Adding a page to this table is a decision with a record behind it, never a way
#: to make a red `docs-corpus` green.
DOCS_DIVERGE: dict[str, PageDiverges] = {
    "control-plane": PageDiverges(
        why=DOCS_DIVERGE_WHY,
        rewrites=(
            (
                """\

`charter init` therefore produces the same plane wherever it runs. Being inside a git repo
no longer changes what you get; it changes only what init *offers*, which is to clone that
repo into your first workspace:

""",
                """\

`charter init` therefore produces the same plane wherever it runs. Being at the top of a git
repo no longer changes what you get; it changes whether init writes anything at all. It
writes nothing, and says what the two ways on are:

""",
            ),
            (
                """\
$ charter init --forge github --owner acme
✓ Initialized control plane (schema 1) → charter.toml, personas/, …
• You are standing in the git repo 'myapp'. Work happens in a workspace, not in the plane
  root — clone it into the first one:
      charter init --clone-this-repo
```

That is an offer, not a prompt: charter never reads stdin (it runs inside hooks, where
blocking would hang the turn), so the second command *is* the acceptance — the same shape
`charter report` uses for consent. Run it and you get `workspaces/default/myapp/`, cloned
from the repo you are standing in and pointed at the same `origin` it has; ignore it and
the plane is complete as it stands. Either way the control plane itself is identical, and
nothing is written to your repo's git state.

""",
                """\
$ charter init --forge github --owner acme
✗ this is the git repo 'myapp', and `charter init` does not make a repository into a
  control plane unless you ask it to. Nothing was written.
• A plane is a directory of its own, and this repo is the first clone in it:
      mkdir ../myapp-plane && cd ../myapp-plane
      charter init --forge github --owner acme
      charter discover && charter clone myapp
• To make THIS repo the plane instead, ask for it by name:
      charter init --plane-is-this-repo --forge github --owner acme
```

That is a refusal, not a prompt: charter never reads stdin (it runs inside hooks, where
blocking would hang the turn), so naming the option *is* the acceptance — the same shape
`charter report` uses for consent. Take the first way and you get
`workspaces/default/myapp/`, cloned from the repo you were standing in and pointed at the
same `origin` it has; take the second and this repo becomes the plane. Either way the
control plane itself is identical, and until you choose, nothing is written to your repo at
all.

**This page describes charter-app**, whose default here is the opposite of the Python
charter's, which scaffolds a plane into the repo and *offers* to clone it into the first
workspace. See [ADR 0035](adr/0035-a-plane-is-untrusted-until-the-operator-opens-it.md) and
charter-app spec decision 27 for why it was reversed. `charter init` anywhere that is not the
top of a git repo is unchanged.

""",
            ),
            (
                """\
A solo user with one repo used to be able to `charter init` and carry on working in that
repo, because `default` *was* the plane root. It no longer is (ADR 0007), so their path is
`charter init --clone-this-repo` — the offer above — and then work in
`workspaces/default/<repo>/`.

""",
                """\
A solo user with one repo used to be able to `charter init` and carry on working in that
repo, because `default` *was* the plane root. It no longer is (ADR 0007), so their path is a
plane in a directory of its own and then `charter clone <repo>` — the first way out of the
refusal above — and then work in `workspaces/default/<repo>/`.

""",
            ),
        ),
    ),
    "install": PageDiverges(
        why=DOCS_DIVERGE_WHY,
        rewrites=(
            (
                """\
`--forge` is `gitlab` (the default) or `github`; `--owner` is the GitLab group or GitHub
org/user whose repos this control plane tracks. Run inside an existing git repo, `init`
also *offers* to clone that repo into your first workspace — accept with `charter init
--clone-this-repo`, because work happens in a workspace, never in the plane root.

""",
                """\
`--forge` is `gitlab` (the default) or `github`; `--owner` is the GitLab group or GitHub
org/user whose repos this control plane tracks. Run at the top of an existing git repo,
`init` writes nothing at all and says so: a plane is a directory of its own and that repo
becomes its first clone (`charter clone <repo>`), because work happens in a workspace, never
in the plane root. To make that repo the plane instead, ask for it by name with `charter
init --plane-is-this-repo`. That default is charter-app's and is the opposite of the Python
charter's, which scaffolds the plane into the repo — ADR 0035, and charter-app spec
decision 27.

""",
            ),
        ),
    ),
}

#: Ask the ORACLE for one page's bytes, the way `_DOCS_DIGEST` asks it for its digests: what a
#: divergence is measured against is the page `docs show` would print, not a file this
#: repository happens to have a copy of.
_DOCS_PAGE = (
    "import sys\n"
    "from charter import docsrc\n"
    "root = docsrc.source()\n"
    "sys.stdout.buffer.write((root / (sys.argv[1] + '.md')).read_bytes())\n"
)


def _oracle_page(topic: str) -> "str | None":
    """What charter's own `docs/` holds for *topic*, or `None` if it could not be read."""
    said = subprocess.run([sys.executable, "-c", _DOCS_PAGE, topic],
                          capture_output=True, text=True)
    return said.stdout if said.returncode == 0 else None


def _opening(words: str) -> str:
    """The first non-empty line of a rewrite's half, for a problem line to quote."""
    return next((line for line in words.splitlines() if line.strip()), words)


def _declared_page_problems(topic: str) -> list[str]:
    """Check one `PageDiverges` in full — see its docstring for why each clause is here."""
    declared = DOCS_DIVERGE[topic]
    problems = []
    if "ADR" not in declared.why or "decision" not in declared.why:
        problems.append(
            f"    {topic}.md: this divergence's `why` names no record — it must cite the ADR "
            "and the spec decision that decided it"
        )
    theirs = _oracle_page(topic)
    if theirs is None:
        return problems + [f"    {topic}.md: the oracle could not print its own page"]
    ours = (DOCS_DIR / f"{topic}.md").read_text()
    if theirs == ours:
        return problems + [
            f"    {topic}.md: the two copies are identical again, so this divergence is over "
            "— delete it and let the digest hold the page"
        ]
    rewritten = theirs
    for was, now in declared.rewrites:
        if was not in rewritten:
            problems.append(
                f"    {topic}.md: charter no longer says {_opening(was)!r}, so this divergence "
                "is a divergence FROM something that is gone"
            )
            continue
        if now not in ours:
            problems.append(
                f"    {topic}.md: charter-app does not say {_opening(now)!r}, which this "
                "divergence declares it says instead"
            )
        rewritten = rewritten.replace(was, now, 1)
    if rewritten != ours:
        problems.append(f"    {topic}.md: differs BEYOND what this divergence declares:")
        problems.extend(
            f"      {line}" for line in difflib.unified_diff(
                rewritten.splitlines(), ours.splitlines(),
                "charter, rewritten as declared", "charter-app", lineterm="", n=1)
        )
    return problems


def _docs_stdout_rewrite(topic: str) -> list:
    """The same pairs, for the scenario that compares what `docs show <topic>` PRINTS.

    Python's `docs show` prints the page's text, so a page's divergence is its render's — and
    driving both off one table is what stops the corpus check and the scenario disagreeing
    about which copy is the right one.
    """
    declared = DOCS_DIVERGE.get(topic)
    if declared is None:
        return []
    return [(was, now, declared.why) for was, now in declared.rewrites]


def check_docs_corpus() -> bool:
    """The vendored `docs/` and the pages the oracle serves are the same pages, byte for byte.

    `check_corpus`'s twin for M2.21, and it exists for the same reason: the per-topic scenarios
    compare what each side PRINTS, so a page that drifts turns one of them red — but a page
    that is missing from this repository is in no scenario at all, and a page charter dropped
    would leave a scenario that still passes against a corpus nobody updated.

    Python's `docs show` prints the file's text, so a digest of the file's bytes is a digest of
    the whole answer. There is no frontmatter here that a rendered body leaves out, which is
    what makes this simpler than `news`'s — the digest is belt to the scenarios' braces only
    for the corpus's SHAPE, not for a part of the page nothing renders.

    **A page in `DOCS_DIVERGE` is compared through its declaration instead of through its
    digest** (charter-app#119) — not skipped: see `PageDiverges` for what is still held, and
    for why skipping was the wrong answer.
    """
    said = subprocess.run([sys.executable, "-c", _DOCS_DIGEST], capture_output=True, text=True)
    if said.returncode != 0:
        print("DIFF docs-corpus: the oracle could not list its pages — " + said.stderr.strip())
        return False
    theirs = dict(line.split() for line in said.stdout.splitlines() if line.strip())
    ours = {
        p.stem: hashlib.sha256(p.read_bytes()).hexdigest()
        for p in sorted(DOCS_DIR.glob("*.md"))
    }
    if not theirs:
        print("DIFF docs-corpus: the oracle ships no pages, so nothing was compared")
        return False
    problems = []
    for topic in sorted(set(theirs) - set(ours)):
        problems.append(f"    only charter has: docs/{topic}.md")
    for topic in sorted(set(ours) - set(theirs)):
        problems.append(f"    only charter-app has: crates/charter-core/docs/{topic}.md")
    both = set(theirs) & set(ours)
    for topic in sorted(both):
        if topic in DOCS_DIVERGE:
            problems.extend(_declared_page_problems(topic))
        elif theirs[topic] != ours[topic]:
            problems.append(f"    differs: {topic}.md")
    for topic in sorted(set(DOCS_DIVERGE) - both):
        # A declaration about a page one side does not carry is a declaration about nothing,
        # and it would otherwise sit here reading as a check.
        problems.append(
            f"    a divergence is declared for {topic}.md, which is not a page both "
            "implementations carry"
        )
    print(("ok   " if not problems else "DIFF ") + f"docs-corpus ({len(theirs)} pages)")
    for line in problems:
        print(line)
    if problems:
        print("    the two copies of charter's documentation have drifted; see "
              "crates/charter-core/docs/SOURCE")
    return not problems


def _docs_topics() -> list[str]:
    """Every topic the vendored corpus carries, in `docsrc.topics()`'s order.

    From the DIRECTORY rather than from the oracle, exactly as `_news_versions` is, so this
    file keeps working whether or not charter is importable in its own process. The
    disagreement that leaves — a topic here that the oracle does not serve — is what
    `check_docs_corpus` is for.
    """
    return sorted(p.stem for p in DOCS_DIR.glob("*.md"))


#: `docs list` and `docs show`, which need no plane and no clock. The `daily` fixture is here
#: only because a scenario runs somewhere; nothing either command prints comes out of it.
DOCS_SCENARIOS = [
    Scenario(
        name="docs-list-names-every-page-this-charter-ships",
        plane="daily",
        python=["docs", "list"],
        pins_the_clock=False,
        stderr_mask=DOCS_SOURCE_MASK,
    ),
    *[
        Scenario(
            # One per topic. A single page would prove the mechanism; one per page is what
            # notices a page whose bytes agree and whose LOOKUP does not — a key built from a
            # filename rather than a stem, or a name with a `-` or a `.` in it that some
            # generator mangled on the way in.
            name=f"docs-show-{topic}",
            plane="daily",
            python=["docs", "show", topic],
            pins_the_clock=False,
            # Empty for every page but the two `DOCS_DIVERGE` names, so this reads as the
            # byte-for-byte comparison it has always been everywhere else.
            stdout_rewrite=_docs_stdout_rewrite(topic),
        )
        for topic in _docs_topics()
    ],
    Scenario(
        name="docs-show-refuses-a-topic-that-is-not-one-and-names-the-real-ones",
        plane="daily",
        # `persona` is a plausible typo for `personas`, and ADR 0009 says charter classifies
        # rather than guessing: the refusal must name the topics, not resolve to the near one.
        python=["docs", "show", "persona"],
        pins_the_clock=False,
        refusal="No charter documentation topic named 'persona'.",
        same_stderr=True,
    ),
    Scenario(
        # The one that is not a typo. `charter docs show ../../etc/passwd` must not be a file
        # read wearing a documentation command; both implementations answer "that is not a
        # topic" and neither opens anything.
        name="docs-show-refuses-a-topic-that-is-a-path-out-of-the-pages",
        plane="daily",
        python=["docs", "show", "../../../../../../etc/passwd"],
        pins_the_clock=False,
        refusal="No charter documentation topic named",
        same_stderr=True,
    ),
]


# M2.12: what `charter version` means when the CLI is not a Python package
# --------------------------------------------------------------------------------------------
#
# ADR 0030. charter's three rows are three facts about a `charter-cp` wheel; two of them have no
# subject for a binary that ships inside the app, and the one number this binary carries of its
# own (`charter-app 0.1.0`) counts a different thing from a pin. So the words differ by
# decision, and what these scenarios compare is the half that must NOT: the **exit status**, in
# each of the three states a script branches on.
#
# **A scenario whose stdout and stderr both carry a `_differs` note asserts less than most here,
# and that is stated rather than hidden.** What is left is real — charter exits 0 with no pin, 0
# when the pin is met and 1 on drift, and a wrapper reading `charter version` behaves the same
# against either implementation — but it is one number, so the three states are all covered
# rather than one standing in for the rest.
#
# **The pin that is MET is `0.62.1` because that is both numbers at once**, and the coincidence
# is load-bearing enough to name: at the oracle's pinned commit `charter.__version__` and the
# newest entry in the vendored news corpus are the same string, so one pin puts both
# implementations in their "pin met" arm. They move together — the corpus and the oracle come
# from one commit — but they are not the same field, and the day they part this scenario reports
# an exit-status difference, which is the right place to find out.

#: The number both implementations call "what is running here": `charter.__version__` on the
#: Python side, the newest entry in the vendored corpus on charter-app's.
PIN_BOTH_SIDES_MEET = "0.62.1"

#: A release neither side is. Old enough that no future bump makes it accidentally current.
PIN_NEITHER_SIDE_MEETS = "0.44.0"

VERSION_ROWS_DIFFER = (
    "charter prints `installed`, `locked` and `latest` — three facts about a charter-cp wheel. "
    "Two have no subject for a binary that ships inside the app, so charter-app prints the "
    "charter release its news corpus reaches, the build carrying it, and the pin (ADR 0030)."
)
VERSION_VERDICT_DIFFERS = (
    "charter's verdict names a wheel and points at `charter version sync`, which cannot reach a "
    "binary inside an app bundle; charter-app says what it brought and how to conform the PLANE. "
    "It deliberately does NOT reuse `in sync with the lock`, which would claim a parity a "
    "partial port does not have (ADR 0030)."
)


def _pinning(version: str):
    """A plane whose `charter.toml` pins *version*, appended to the fixture's own manifest.

    Appended rather than rewritten: the rest of the manifest decides the forge, the persona and
    the memory share, and a scenario that replaced it would be comparing two commands on a
    plane no fixture describes.
    """

    def setup(root: Path) -> None:
        manifest = root / "charter.toml"
        manifest.write_text(f'{manifest.read_text()}\n[charter]\nversion = "{version}"\n')

    return setup


VERSION_SCENARIOS = [
    Scenario(
        name="version-on-a-plane-that-pins-nothing-is-not-drift",
        plane="daily",
        python=["version"],
        pins_the_clock=False,
        stdout_differs=VERSION_ROWS_DIFFER,
        stderr_differs=VERSION_VERDICT_DIFFERS,
    ),
    Scenario(
        name="version-on-a-plane-pinning-what-both-sides-are-is-not-drift",
        plane="daily",
        setup=_pinning(PIN_BOTH_SIDES_MEET),
        python=["version"],
        pins_the_clock=False,
        stdout_differs=VERSION_ROWS_DIFFER,
        stderr_differs=VERSION_VERDICT_DIFFERS,
    ),
    Scenario(
        # The one that must agree, and the reason the other two are here: an exit 1 that only
        # one implementation gives turns a wrapper's `charter version || conform` into a no-op
        # on the other.
        #
        # `refusal` rather than `stderr_differs`, and it says more than the note would: the
        # harness treats a Python exit that a scenario has not declared as a defect in the
        # scenario, and the substring it then requires of the RUST stderr is one both sides
        # write. So what is compared here is that both implementations call this state drift
        # and both name the pin — the sentences around that phrase are each their own.
        name="version-on-a-plane-pinning-a-release-neither-side-is-exits-one",
        plane="daily",
        setup=_pinning(PIN_NEITHER_SIDE_MEETS),
        python=["version"],
        pins_the_clock=False,
        stdout_differs=VERSION_ROWS_DIFFER,
        refusal=f"drift: this control plane pins {PIN_NEITHER_SIDE_MEETS}",
    ),
]


# M6.5: the alert rows — `statusline._alerts`, ported as `crates/charter-core/src/alerts.rs`
# --------------------------------------------------------------------------------------------
#
# charter draws its alert rows full width below zone 2: a pin the running charter does not meet,
# a front door naming no persona, workspaces needing a reinit, a nested plane, and a plane root
# being worked in. Each carries the command that fixes it, and each renders only when real.
#
# These scenarios compare the ROWS, picked out of both stdouts by `ALERT_MARK` (see
# `Scenario.alerts`), and keep comparing zone 1 above the rule exactly as the statusline
# scenarios above do. Every scenario states how many rows charter draws, so one that stops
# producing the row it is named for fails rather than comparing two empty lists.
#
# What is NOT here, and where it is instead: the nested-plane row needs the plane copy to sit
# inside another plane's `workspaces/`, which `_refuse_enclosing_plane` exists to make
# impossible for this harness — it is `alerts.rs`'s own tests that drive it.

#: One decided difference, and only one. charter's pin row points at `charter version sync`,
#: which INSTALLS a published charter-cp; charter-app is a binary inside the app and answers
#: that verb with a refusal whose last line is "what this charter is, and what this plane
#: pins:  charter version". So charter-app's row names `charter version` directly — the command
#: that prints both numbers and how to conform the plane — rather than a verb that refuses and
#: then points at it.
PIN_ROW_REMEDY = (
    "· charter version sync",
    "· charter version",
    "ADR 0030: `charter version sync` moves a published charter-cp release, which charter-app "
    "is not; its row names the command that says what this charter brought and how to "
    "conform the plane, which is where `version sync` itself sends the operator here.",
)


def _manifest_says(old: str, new: str):
    """`charter.toml` with *old* replaced by *new* — the fixture's own manifest, edited, so the
    rest of it still decides the forge, the persona and the memory share."""

    def setup(root: Path) -> None:
        manifest = root / "charter.toml"
        text = manifest.read_text()
        if old not in text:
            raise SystemExit(f"setup: the fixture manifest has no {old!r} to replace")
        manifest.write_text(text.replace(old, new))

    return setup


def _manifest_ends_with(extra: str):
    def setup(root: Path) -> None:
        manifest = root / "charter.toml"
        manifest.write_text(f"{manifest.read_text()}\n{extra}")

    return setup


def _stale(ws: str):
    """Stamp *ws* with an older layout version: a workspace needing `charter ws reinit`."""

    def setup(root: Path) -> None:
        (root / "workspaces" / ws / ".charter-structure").write_text("4\n")

    return setup


def _a_plane_root_repo(root: Path) -> None:
    """The plane root as a git repository on `main`, one commit, no remote, clean.

    No `origin`: the default branch is then the local `main`, which is the fallback charter
    reads straight off the refs — and a remote would have to be one no forge answers, which is
    `_forge_trap`'s business and not this row's. `notes.md` is a tracked file for a scenario to
    dirty; `.charter/` is ignored, as `charter init` writes it.
    """
    side = root.parent
    _identity(side)
    (root / ".gitignore").write_text(".charter/\n")
    (root / "notes.md").write_text("# notes\n")
    _git(side, "init", "-q", "-b", "main", ".", cwd=root)
    _git(side, "add", "-A", cwd=root)
    _git(side, "commit", "-q", "-m", "the plane", cwd=root)


def _root_dirty(root: Path) -> None:
    _a_plane_root_repo(root)
    (root / "notes.md").write_text("# notes\n\nan edit nobody committed\n")


def _root_with_only_untracked_files(root: Path) -> None:
    """Memory defaults to `share = "local"`, so every plane a few days old carries untracked
    files. They are not the root being worked in — `doctor` asks git `-uno` for the same
    reason — so this is NOT an alert."""
    _a_plane_root_repo(root)
    (root / "personas" / "steward" / "memory" / "a-local-memory.md").write_text("# local\n")


def _root_off_its_branch(root: Path) -> None:
    _a_plane_root_repo(root)
    _git(root.parent, "checkout", "-q", "-b", "side", cwd=root)


def _root_detached(root: Path) -> None:
    _a_plane_root_repo(root)
    _git(root.parent, "checkout", "-q", "--detach", cwd=root)


def _push_record(root: Path, **record: str) -> None:
    head = _git(root.parent, "rev-parse", "HEAD", cwd=root)
    state = root / ".charter"
    state.mkdir(parents=True, exist_ok=True)
    (state / "plane-push.json").write_text(json.dumps({"head": head, **record}))


def _root_with_a_memory_commit_never_pushed(root: Path) -> None:
    """A push record whose commit reached no remote: there is no upstream to be an ancestor
    of, so the record is still true, and the next `git reset --hard` would delete it."""
    _a_plane_root_repo(root)
    _push_record(root, outcome="rejected", branch="main")


def _root_with_a_memory_commit_awaiting_a_pull_request(root: Path) -> None:
    """The same, landed on `charter/<sha>` because `main` requires a pull request: nothing is
    at risk, something is unfinished — so it is the WARN colour, not the red one."""
    _a_plane_root_repo(root)
    _push_record(root, outcome="branched", landed="charter/1a2b3c4d", branch="main")


def _root_ahead_of_its_upstream(root: Path) -> None:
    """The plane root with one commit its upstream has and one it does not.

    What **A3b** needs, and nothing less does. That guard "only speaks when it has measured that
    something really would be lost", and the measurement is
    `git rev-list --count HEAD --not <target> @{upstream}` — so the root needs a real upstream
    ref and a real commit ahead of it. `_a_plane_root_repo` deliberately has no remote, which
    makes `@{upstream}` fail and the guard silent: a scenario built on it is green while
    measuring nothing, which is how it was first written here.

    The remote is a bare repository BESIDE the plane copy: `file://` needs no network and no
    credential, and it sits outside the tree the scenario compares. `.git` is in
    `ROOT_REPO_IGNORES`, so the two sides' different absolute remote paths are not compared.
    """
    _a_plane_root_repo(root)
    side = root.parent
    remote = side / "origin.git"
    _git(side, "init", "-q", "--bare", str(remote))
    _git(side, "remote", "add", "origin", str(remote), cwd=root)
    _git(side, "push", "-q", "-u", "origin", "main", cwd=root)
    (root / "notes.md").write_text("# notes\n\na commit that reached no remote\n")
    _git(side, "add", "-A", cwd=root)
    _git(side, "commit", "-q", "-m", "unpushed", cwd=root)


def _root_dirty_off_its_branch_with_a_memory_never_pushed(root: Path) -> None:
    """Every finding at once: they share ONE row, in charter's order."""
    _root_off_its_branch(root)
    (root / "notes.md").write_text("# notes\n\nan edit nobody committed\n")
    _push_record(root, outcome="rejected", branch="main")


#: What a plane root that is a repository leaves that the tree walk cannot compare: git's own
#: files, and charter's repo-state TTL cache, which charter-app does not keep (it asks git on
#: the render, as the app's panels do, and writes nothing on a read).
ROOT_REPO_IGNORES = {
    ".git": "the index and the reflogs carry timestamps and inodes; what the row says about "
    "the tree is what is compared",
    ".charter/cache": "where charter keeps its TTL cache of `git status` answers "
    "(`repostate.json`), a directory it creates to hold it; charter-app keeps none, because a "
    "status line render is a read and writes nothing",
}


def _alert(name: str, alerts: int, *, setup=None, turn: str = A_TURN, cols: int = 120,
           rewrite: "tuple[str, str, str] | None" = None,
           ignore: "dict[str, str] | None" = None) -> Scenario:
    return Scenario(
        name=f"statusline-alerts-{name}",
        plane="daily",
        python=["statusline"],
        stdin=turn,
        env=_pane(cols),
        setup=setup,
        stdout_cut_at=IDENTITY_ROW,
        stdout_cut_why=BELOW_THE_RULE,
        alerts=alerts,
        alert_rewrite=rewrite,
        ignore=ignore or {},
    )


ALERT_SCENARIOS = [
    # The healthy plane. Rows render only when real, so a plane with nothing wrong costs none —
    # and the count of zero is asserted against charter, not assumed.
    _alert("none-on-a-healthy-plane", 0),
    # A pin neither side meets: charter's row, with the one decided difference in its remedy.
    _alert("a-pin-this-charter-does-not-meet", 1, setup=_pinning(PIN_NEITHER_SIDE_MEETS),
           rewrite=PIN_ROW_REMEDY),
    # A pin both sides meet is not drift, and draws nothing.
    _alert("a-pin-this-charter-meets", 0, setup=_pinning(PIN_BOTH_SIDES_MEET)),
    # A pin beside `[update] channel = "dev"` is its own state with its own row (#1018): the
    # drift row's `version sync` would install the pin over a plane following `main`.
    _alert("a-pin-beside-the-dev-channel", 1, setup=_manifest_ends_with(
        f'[charter]\nversion = "{PIN_NEITHER_SIDE_MEETS}"\n\n[update]\nchannel = "dev"\n')),
    # A channel charter does not know is `stable` — the conservative one — so this is the
    # ordinary drift row and not the dev one.
    _alert("a-pin-beside-a-channel-charter-does-not-know", 1, setup=_manifest_ends_with(
        f'[charter]\nversion = "{PIN_NEITHER_SIDE_MEETS}"\n\n[update]\nchannel = "DEV"\n'),
        rewrite=PIN_ROW_REMEDY),
    # A front door that names nothing: the persona chip just disappears, and this row is what
    # makes the absence a message.
    _alert("a-front-door-naming-no-persona", 1,
           setup=_manifest_says('default = "steward"', 'default = "ghost"')),
    # `str(val)`: a front door that is not a string is still quoted back as charter prints it.
    _alert("a-front-door-that-is-a-number", 1,
           setup=_manifest_says('default = "steward"', "default = 7")),
    # A blank front door is no front door, not a persona named "   ".
    _alert("a-front-door-that-is-blank", 0,
           setup=_manifest_says('default = "steward"', 'default = "   "')),
    # Another workspace stale: `reinit 1 ws`.
    _alert("another-workspace-needing-a-reinit", 1, setup=_stale("beta")),
    # The ACTIVE workspace stale is the identity row's tip, not an alert — the two surfaces do
    # not both say it. With `beta` stale too the count is 1, not 2.
    _alert("the-active-workspace-is-the-identity-rows-to-flag", 1,
           setup=_both(_stale("alpha"), _stale("beta")), turn=A_TURN_IN_ALPHA),
    _alert("the-active-workspace-alone-draws-no-alert", 0, setup=_stale("alpha"),
           turn=A_TURN_IN_ALPHA),
    # charter's ONE guard: a `persona` that is not a table raises inside `_alerts`, and every
    # row after the raise is dropped — the stale workspace's among them — while the pin row
    # before it stands.
    _alert("a-raise-drops-the-rows-after-it-and-not-the-rows-before", 1, setup=_both(
        _manifest_says('[persona]\ndefault = "steward"\n', ""),
        _manifest_says("schema = 1\n", 'schema = 1\npersona = "steward"\n'),
        _manifest_ends_with(f'[charter]\nversion = "{PIN_NEITHER_SIDE_MEETS}"\n'),
        _stale("beta"),
    ), rewrite=PIN_ROW_REMEDY),
    # The plane root. Clean on its default branch: nothing.
    _alert("a-plane-root-that-is-clean", 0, setup=_a_plane_root_repo, ignore=ROOT_REPO_IGNORES),
    _alert("a-plane-root-with-only-untracked-files", 0, setup=_root_with_only_untracked_files,
           ignore=ROOT_REPO_IGNORES),
    _alert("a-dirty-plane-root", 1, setup=_root_dirty, ignore=ROOT_REPO_IGNORES),
    _alert("a-plane-root-off-its-default-branch", 1, setup=_root_off_its_branch,
           ignore=ROOT_REPO_IGNORES),
    _alert("a-plane-root-on-a-detached-head", 1, setup=_root_detached,
           ignore=ROOT_REPO_IGNORES),
    _alert("a-plane-root-holding-a-memory-commit-never-pushed", 1,
           setup=_root_with_a_memory_commit_never_pushed, ignore=ROOT_REPO_IGNORES),
    _alert("a-plane-root-holding-a-memory-commit-awaiting-a-pull-request", 1,
           setup=_root_with_a_memory_commit_awaiting_a_pull_request, ignore=ROOT_REPO_IGNORES),
    _alert("a-plane-root-with-every-finding-on-one-row", 1,
           setup=_root_dirty_off_its_branch_with_a_memory_never_pushed,
           ignore=ROOT_REPO_IGNORES),
    # Every row at once, in charter's order: the order IS what a reader meets first.
    _alert("every-row-in-charters-order", 4, setup=_both(
        _pinning(PIN_NEITHER_SIDE_MEETS),
        _manifest_says('default = "steward"', 'default = "ghost"'),
        _stale("beta"),
        _root_dirty,
    ), rewrite=PIN_ROW_REMEDY, ignore=ROOT_REPO_IGNORES),
    # …in a pane too narrow for them: each row is cropped with `…` at the frame's inner width,
    # measured in columns, and the border still lines up.
    _alert("cropped-to-a-narrow-pane", 3, setup=_both(
        _manifest_says('default = "steward"', 'default = "ghost"'),
        _stale("beta"),
        _root_with_a_memory_commit_never_pushed,
    ), cols=44, ignore=ROOT_REPO_IGNORES),
    # A plane that recolours its accents: the `⚠` of a warn row and of a bad one, and the red
    # word inside the plane-root row, all follow `[frame]`.
    _alert("in-the-colours-the-plane-chose", 2, setup=_both(
        _manifest_ends_with('[frame]\nwarn = "magenta"\nbad = "brightcyan"\n'),
        _stale("beta"),
        _root_with_a_memory_commit_never_pushed,
    ), ignore=ROOT_REPO_IGNORES),
]


# --------------------------------------------------------------------------- #
# `charter hook pretooluse`: the Bash guard, as a PROCESS, on both sides         #
# --------------------------------------------------------------------------- #
#
# M3.1 stage 6. Every other differential in this repository compares a guard ARM against the
# Python function it was ported from (`tests/differential/shellseg.py`, `planeroot.py`). These
# compare the whole hook: the same payload on stdin, the same fixture plane, and then the exit
# status, the JSON on stdout byte for byte, and the two planes afterwards.
#
# **They are what measures the ORDER.** Eight arms, and which of two that both fire is the one
# the chat is told about is a fact about `pretooluse` rather than about any arm — no per-function
# harness can see it, and it is the whole subject of `toolgate.rs`. `denies` names the arm by the
# sentence it opens with, checked on BOTH sides, so an order that drifted on either shows up as
# the wrong sentence rather than as a silent agreement.
#
# **The bookkeeping is IGNORED, narrowly and by name.** `pretooluse` writes three things the port
# does not: a guard sighting, the persona tool-gate's session snapshot, and a trace row. None
# changes a verdict, all are declared gaps in `toolgate.rs`'s header, and each is listed with the
# reason rather than the whole of `.charter/` being waved through — a wide ignore here would hide
# the arms that really do write, which is what a guard differential must not do.
GUARD_IGNORES = {
    ".charter/guard-seen.json": "`_mark_guard_seen` — the sighting `doctor` and the status line "
    "read back to say the guard is live under this harness. Not ported: it is a fact about a "
    "plane's bookkeeping, not a verdict (charter-core `toolgate`'s header lists it)",
    ".charter/sessions": "`_turn_bump` and the persona tool-gate's per-session snapshot "
    "(`<sid>.tools`, `<sid>.gate`, charter#432). The ALLOW half of `pretooluse` is not ported "
    "at all, so nothing here has a snapshot to freeze",
    ".charter/persona-state": "`_trace` — one row per verdict. The `Verdict` carries its "
    "`reason` and `shape` so a later stage can write them; nothing writes them yet",
}

#: The plane root, as a denial that names it spells it. Each side's copy is at its own absolute
#: path, so the PATH is what neither implementation decides; everything the denial says around it
#: still has to match byte for byte.
GUARD_PLANE_PATH = (r"(?<=git -C )\S+", "each side's plane copy lives at its own absolute path")

#: The payload a harness sends, with only the fields the guard reads.
#:
#: **`cwd` defaults to `"."`, and that is not a placeholder.** It is where the COMMAND would run,
#: which A, A3 and A3b all resolve their paths against — and the plane root is a different
#: absolute path on each side (`<scratch>/python/plane` against `<scratch>/rust/plane`), so no
#: literal could name it in a payload both sides are handed. Both processes stand in their own
#: root, so `"."` is the same answer computed on each side. A scenario that needs the command to
#: run somewhere else passes `cwd=` itself.
def _tool_call(command: str, **extra) -> str:
    return json.dumps({
        "session_id": SESSION,
        "cwd": ".",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": command},
        **extra,
    })


def _guard(name: str, command: str, *, denies: str = "", allows: bool = False,
           plane: str = "daily", setup=None, ignore=None, env=None,
           stdout_mask=None, local_origin_why: str = "", **payload) -> Scenario:
    """One `charter hook pretooluse`, put to both implementations.

    `$CHARTER_HARNESS` is set on every one of them: A7 reads it to decide whether an `agent_id`
    means a sub-agent, and a scenario that left it to the environment would answer differently
    on a developer's machine than in CI.
    """
    return Scenario(
        name=f"pretooluse-{name}",
        plane=plane,
        setup=setup,
        python=["hook", "pretooluse"],
        stdin=_tool_call(command, **payload),
        # A guard reads a command line and a payload; it takes no clock and writes nothing that
        # carries a stamp, so `hook` has no `--now` to be given one.
        pins_the_clock=False,
        denies=denies,
        allows=allows,
        stdout_mask=stdout_mask or [],
        local_origin_why=local_origin_why,
        ignore={**GUARD_IGNORES, **(ignore or {})},
        env={"CHARTER_HARNESS": "claude-code", **(env or {})},
    )


PRETOOLUSE_SCENARIOS = [
    # ---- the two answers that are not a denial at all. First, because every scenario below
    # would pass against a guard that refused everything.
    _guard("allows-an-ordinary-command", "git status", allows=True),
    _guard("allows-the-canonical-handoff",
           "charter handoff beta <<'BRIEF'\nship it\nBRIEF", allows=True),

    # ---- A: the leak guard. Ungated, and first.
    _guard("leak", "cat .charter/vaults/db.json",
           denies="reads a vault/secret file directly"),
    # ---- A2: the golden rule.
    _guard("golden-rule", "git clone git@github.com:o/r.git",
           denies="The control plane is **token-only**"),
    # ---- A3 and A3b: the plane root is one shared working tree. Both need a real repository
    # underneath, which is what `_a_plane_root_repo` is for.
    _guard("plane-root-branch", "git checkout -b side",
           setup=_a_plane_root_repo, ignore=ROOT_REPO_IGNORES,
           denies="The plane root is one working tree every session shares"),
    _guard("plane-root-reset", "git reset --hard HEAD~1",
           setup=_root_ahead_of_its_upstream, ignore=ROOT_REPO_IGNORES,
           # A3b's denial names the root so the operator can look at what would go, and each
           # side's root is its own absolute path. The masked comparison is still exact
           # everywhere else, which is where the count and the upstream's name are.
           stdout_mask=[GUARD_PLANE_PATH],
           local_origin_why="A3b's whole condition is `@{upstream}`, so the root needs a "
                            "remote it is ahead of. It is a bare repository beside the plane "
                            "copy: no forge is involved and none is being measured",
           denies="that is not on origin/main"),
    # ...and the half that says A3b is a MEASUREMENT and not a word match: the same command in
    # the same root, with nothing ahead of the upstream, is allowed.
    _guard("plane-root-reset-with-nothing-at-risk", "git reset --hard HEAD~1",
           setup=_a_plane_root_repo, ignore=ROOT_REPO_IGNORES, allows=True),
    # ---- A4: an unattended run may not publish. Both sides of the gate, because "unattended"
    # is the whole condition and an attended answer that started denying is a guard that
    # reached the operator.
    _guard("release-floor", "gh release create v1.0.0",
           permission_mode="bypassPermissions",
           denies="Publishing is on charter's floor"),
    _guard("release-floor-is-attended-only", "gh release create v1.0.0", allows=True),
    # ---- A5 and A6: a live substitution in prose that gets published.
    _guard("forge-substitution", 'gh issue create --body "$(cat notes)"',
           denies="`gh issue create` publishes prose"),
    _guard("charter-substitution", 'charter persona remember devops "$(cat notes)"',
           denies="`charter persona remember` takes text"),
    # ---- A7: all four rows.
    _guard("handoff-spelling", "python3 -m charter handoff beta <<'BRIEF'\nx\nBRIEF",
           denies="must be spelled exactly that"),
    _guard("handoff-brief-source", "charter handoff beta",
           denies="takes its brief from a QUOTED heredoc"),
    _guard("handoff-shell-string", "eval 'charter handoff beta'",
           denies="inside a string or a heredoc a shell runs"),
    _guard("handoff-subagent", "charter handoff beta <<'BRIEF'\nx\nBRIEF",
           agent_id="sub-1",
           denies="refused from inside a sub-agent"),
    _guard("handoff-unattended", "charter handoff beta <<'BRIEF'\nx\nBRIEF",
           permission_mode="bypassPermissions",
           denies="refused in an unattended run"),
    # ...and the harness nobody measured, where an `agent_id` means nothing and the canonical
    # spelling still goes through. The gap this pins is a REFUSAL that must not happen.
    _guard("handoff-from-an-unmeasured-harness",
           "charter handoff beta <<'BRIEF'\nship it\nBRIEF",
           agent_id="sub-1", env={"CHARTER_HARNESS": "opencode"}, allows=True),

    # ---- the ORDER, which is the only thing no per-arm harness can see.
    # A before everything: a line that leaks AND hands off is explained by the leak.
    _guard("leak-outranks-the-handoff-guard",
           "cat .charter/vaults/db.json && charter handoff beta",
           denies="reads a vault/secret file directly"),
    # A5 before A6: a line that is both is explained by the guard that publishes to a forge.
    _guard("the-forge-guard-outranks-charters-own",
           'charter persona remember d "$(x)" && gh issue create --body "$(x)"',
           denies="`gh issue create` publishes prose"),
    # A7's own order: a sub-agent is asked before an unattended run, and both before the
    # spelling. One command, three payloads, three different sentences.
    _guard("a-sub-agent-is-asked-before-an-unattended-run",
           "python3 -m charter handoff beta",
           agent_id="sub-1", permission_mode="bypassPermissions",
           denies="refused from inside a sub-agent"),
    _guard("an-unattended-run-is-asked-before-the-spelling",
           "python3 -m charter handoff beta",
           permission_mode="bypassPermissions",
           denies="refused in an unattended run"),

    # ---- the plane gate. `plane=""` is a directory that is not a plane, which is where the
    # five gated arms denied in every unrelated repository on the machine (charter#852).
    _guard("outside-a-plane-the-golden-rule-is-silent", "git clone git@github.com:o/r.git",
           plane="", allows=True),
    _guard("outside-a-plane-the-handoff-guard-is-silent", "charter handoff beta",
           plane="", allows=True),
    # ...and the two that are NOT gated, in the same directory: a fact about the shell is a
    # fact about the shell wherever it is typed.
    _guard("outside-a-plane-the-charter-prose-guard-still-refuses",
           'charter persona remember devops "$(cat notes)"',
           plane="", denies="`charter persona remember` takes text"),
    _guard("outside-a-plane-the-leak-guard-still-refuses", "cat .charter/vaults/db.json",
           plane="", denies="reads a vault/secret file directly"),

    # ---- the payloads a guard has to survive. A hook that crashed on one of these would
    # block every tool call in the session it was armed on.
    _guard("a-payload-with-no-command", "", allows=True),
]

# Exactly one of the two, on every one of them: a guard scenario that asserts neither is a
# scenario where both sides can allow and nothing is proved. This is the `refusal` field's own
# objection, applied to the field that replaces it.
for _s in PRETOOLUSE_SCENARIOS:
    assert bool(_s.denies) != _s.allows, f"{_s.name} says neither what it denies nor that it allows"


SCENARIOS = [
    *PRETOOLUSE_SCENARIOS,
    *INIT_SCENARIOS,
    *LADDER_SCENARIOS,
    *NEWS_SCENARIOS,
    *M28_SCENARIOS,
    Scenario(
        name="harness-list-refuses-every-name-that-is-a-charter-command",
        plane="minimal",
        python=["harness", "list"],
        pins_the_clock=False,
        setup=_declare_a_profile_per_command_word,
        stderr_cut_at=HARNESS_LIST_REGISTRY_CUT,
        stderr_cut_why=HARNESS_LIST_REGISTRY_SECTION,
        ignore={"charter.local.toml": "written by this scenario's own setup"},
    ),
    Scenario(
        name="harness-list-on-a-local-file-that-will-not-parse",
        plane="minimal",
        python=["harness", "list"],
        pins_the_clock=False,
        setup=_malform_the_local_file,
        stderr_cut_at=HARNESS_LIST_REGISTRY_CUT,
        stderr_cut_why=HARNESS_LIST_REGISTRY_SECTION,
        stderr_mask=[TOML_DIAGNOSTIC],
        ignore={"charter.local.toml": "written by this scenario's own setup"},
    ),
    Scenario(
        name="harness-list-reads-the-same-profiles-and-refuses-the-same-ones",
        plane="daily",
        python=["harness", "list"],
        pins_the_clock=False,
        setup=_declare_harness_profiles,
        stderr_cut_at=HARNESS_LIST_REGISTRY_CUT,
        stderr_cut_why=HARNESS_LIST_REGISTRY_SECTION,
        ignore={
            "charter.local.toml": "written by this scenario's own setup, into both copies",
        },
    ),
    Scenario(
        name="harness-list-on-a-plane-that-declares-nothing",
        plane="minimal",
        python=["harness", "list"],
        pins_the_clock=False,
        stderr_cut_at=HARNESS_LIST_REGISTRY_CUT,
        stderr_cut_why=HARNESS_LIST_REGISTRY_SECTION,
    ),
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
        name="remember",
        plane="daily",
        python=["workspace", "remember", "The importer drops rows over 4 MB", "-w", "alpha",
                "--no-sync"],
    ),
    Scenario(
        name="remember-a-multi-line-fact",
        plane="daily",
        python=["workspace", "remember", "Retries are capped at 3\nand the 4th is dropped",
                "-w", "alpha", "--no-sync"],
    ),
    Scenario(
        name="remember-into-an-empty-journal",
        plane="daily",
        python=["workspace", "remember", "Nothing was here before", "-w", "beta", "--no-sync"],
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
        refusal="outside the directories",
    ),
    Scenario(
        name="remember-through-two-hops-out-of-the-plane",
        plane="daily",
        setup=_two_hop_out_of_the_plane,
        python=["workspace", "remember", "ssh-rsa AAAA attacker@example.com", "-w", "alpha",
                "--no-sync"],
        refusal="outside the directories",
    ),
    Scenario(
        name="remember-through-a-dangling-memory-index-link",
        plane="daily",
        setup=_dangle_a_memory_index_out_of_the_plane,
        python=["workspace", "remember", "A durable fact", "-w", "alpha", "--no-sync"],
        refusal="outside the directories",
    ),
    Scenario(
        name="a-todo-linked-out-of-the-plane-is-not-read-as-a-duplicate",
        plane="daily",
        setup=_plant_a_todo_linked_out_of_the_plane,
        # The text matches the planted file's heading exactly. If the entry is read, the
        # duplicate check refuses and echoes that heading; if it is not, the todo records
        # normally — which is what charter does.
        python=["ws", "todo", "Board minutes: layoffs in Q3", "-w", "alpha"],
        stderr_differs=CONFIRMS_ON_STDERR,
    ),
    Scenario(
        name="a-duplicate-todo-is-refused",
        plane="daily",
        # `alpha` already has "Review the rollout plan". Duplicate INTENT is refused because
        # closing one of a near-identical pair leaves its twin looking outstanding.
        python=["ws", "todo", "Review the rollout plan", "-w", "alpha"],
        refusal="already on the list",
    ),
    *REPO_SCENARIOS,
    *STATUS_SCENARIOS,
    *GL_REFRESH_SCENARIOS,
    *STATUSLINE_SCENARIOS,
    *ALERT_SCENARIOS,

    # --- M2.2: `charter recall`. What an agent reads at session start; byte for byte.
    Scenario(
        name="recall-lists-every-base-newest-first",
        plane="daily",
        python=["recall", *RECALL_OWNERS],
    ),
    Scenario(
        name="recall-a-query-ranks-and-labels-each-hit",
        plane="daily",
        python=["recall", "plane", *RECALL_OWNERS, "--full"],
    ),
    Scenario(
        name="recall-every-workspace-with-a-limit-says-what-it-cut",
        plane="daily",
        python=["recall", "--all-workspaces", "--persona", "devops", "--limit", "2"],
    ),
    Scenario(
        name="recall-a-query-of-only-stopwords-searched-nothing",
        plane="daily",
        python=["recall", "in the", *RECALL_OWNERS],
    ),
    Scenario(
        name="recall-a-query-that-matches-nothing",
        plane="daily",
        python=["recall", "zzz", "-w", "beta", "--persona", "devops"],
    ),
    Scenario(
        name="recall-since-a-date-that-excludes-everything",
        plane="daily",
        python=["recall", "--since", "2026-04-01", *RECALL_OWNERS],
    ),
    Scenario(
        name="recall-since-an-age-counts-the-refs-it-could-not-date",
        plane="daily",
        python=["recall", "--since", "90d", *RECALL_OWNERS, "--scope", "workspace,refs"],
    ),
    Scenario(
        name="recall-refuses-a-scope-it-does-not-have",
        plane="daily",
        python=["recall", "--scope", "bogus", "-w", "alpha"],
        refusal="invalid --scope",
        same_stderr=True,
    ),
    Scenario(
        name="recall-refuses-a-persona-name-outside-the-alphabet",
        plane="daily",
        python=["recall", "--persona", "Nope", "-w", "alpha"],
        refusal="invalid persona name",
        same_stderr=True,
    ),
    Scenario(
        name="recall-refuses-a-persona-the-plane-does-not-define",
        plane="daily",
        python=["recall", "--persona", "nope", "-w", "alpha"],
        refusal="no persona 'nope'",
        same_stderr=True,
    ),
    Scenario(
        name="recall-refuses-a-since-it-cannot-read",
        plane="daily",
        python=["recall", "--since", "yesterday", *RECALL_OWNERS],
        refusal="unrecognised --since",
        same_stderr=True,
    ),
    Scenario(
        name="recall-the-sessions-ephemeral-scratch",
        plane="daily",
        setup=_ephemeral_scratch,
        python=["recall", "--persona", "devops", "--scope", "ephemeral"],
    ),
    Scenario(
        name="recall-searches-no-entry-the-gate-refuses",
        plane="daily",
        # `leak.md` links out of the plane and matches the query in its heading and body:
        # read, it would rank first. Refused, the search finds only the journal's own entry,
        # and the link that lands INSIDE the plane — labelled by where it lands.
        setup=_entries_the_gate_refuses,
        python=["recall", "mondays", "-w", "alpha", "--scope", "workspace"],
    ),
    Scenario(
        name="recall-lists-no-entry-the-gate-refuses",
        plane="daily",
        setup=_entries_the_gate_refuses,
        python=["recall", "-w", "alpha", "--scope", "workspace", "--limit", "0", "--full"],
    ),
    Scenario(
        name="recall-a-query-only-a-refused-entry-matches-finds-nothing",
        plane="daily",
        setup=_entries_the_gate_refuses,
        python=["recall", "board", "-w", "alpha", "--scope", "workspace"],
    ),
    Scenario(
        name="recall-walks-nested-refs-and-not-a-link-out-of-the-plane",
        plane="daily",
        setup=_nested_refs,
        python=["recall", "--persona", "devops", "--scope", "refs", "--full", "--limit", "0"],
    ),
    Scenario(
        name="recall-dates-by-stamp-then-filename",
        plane="daily",
        setup=_dates_every_way,
        python=["recall", "-w", "alpha", "--scope", "workspace", "--limit", "0", "--full"],
    ),
    Scenario(
        name="recall-since-narrows-a-query-without-reranking-it",
        plane="daily",
        setup=_dates_every_way,
        python=["recall", "mondays", "-w", "alpha", "--scope", "workspace",
                "--since", "2026-03-15"],
    ),
    Scenario(
        name="recall-names-a-base-it-could-not-read",
        plane="daily",
        setup=_journal_unreadable,
        python=["recall", *RECALL_OWNERS],
        refusal="cannot be checked",
        same_stderr=True,
    ),
    # --- `charter workspace recall | remember | note | forget`.
    Scenario(
        name="workspace-recall-lists-the-journal",
        plane="daily",
        python=["workspace", "recall", "-w", "alpha"],
    ),
    Scenario(
        name="workspace-recall-a-query",
        plane="daily",
        python=["workspace", "recall", "-w", "alpha", "-q", "mondays"],
    ),
    Scenario(
        name="workspace-recall-a-query-that-matches-nothing",
        plane="daily",
        python=["workspace", "recall", "-w", "alpha", "-q", "zzz"],
    ),
    Scenario(
        name="workspace-recall-an-empty-journal",
        plane="daily",
        python=["workspace", "recall", "-w", "beta"],
    ),
    Scenario(
        name="workspace-remember-with-no-text-lists-the-journal",
        plane="daily",
        python=["workspace", "remember", "-w", "alpha"],
    ),
    Scenario(
        name="workspace-note-with-no-text-lists-the-journal",
        plane="daily",
        python=["workspace", "note", "-w", "alpha"],
    ),
    Scenario(
        name="workspace-remember-under-a-title-of-its-own",
        plane="daily",
        python=["workspace", "remember", "Fact with title", "--title", "  Custom T  ",
                "-w", "alpha", "--no-sync"],
    ),
    Scenario(
        name="workspace-note-records-like-remember",
        plane="daily",
        python=["workspace", "note", "A note", "-w", "alpha", "--no-sync"],
    ),
    Scenario(
        name="workspace-remember-on-a-local-workspace-says-it-stays-on-disk",
        plane="daily",
        python=["workspace", "remember", "Fact", "-w", "beta"],
    ),
    Scenario(
        name="workspace-remember-on-a-live-workspace-with-memory-kept-local",
        plane="daily",
        python=["workspace", "remember", "Fact", "-w", "alpha"],
    ),
    Scenario(
        name="workspace-forget-by-slug",
        plane="daily",
        python=["workspace", "forget", "the-api-returns-418-on-mondays", "-w", "alpha"],
    ),
    Scenario(
        name="workspace-forget-by-filename",
        plane="daily",
        python=["workspace", "forget", "20260302-091500-closed-todo-write-the-migration.md",
                "-w", "alpha"],
    ),
    Scenario(
        name="workspace-forget-a-slug-nothing-matches",
        plane="daily",
        python=["workspace", "forget", "nothing-here", "-w", "alpha"],
        refusal="no memory 'nothing-here'",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-forget-a-fifo-is-not-a-memory",
        plane="daily",
        setup=_a_fifo_in_the_journal,
        python=["workspace", "forget", "pipe", "-w", "alpha"],
        refusal="no memory 'pipe'",
        same_stderr=True,
    ),
    Scenario(
        name="workspace-forget-a-link-out-of-the-plane-deletes-nothing",
        plane="daily",
        setup=_a_journal_entry_linked_out,
        python=["workspace", "forget", "leak", "-w", "alpha"],
        refusal="no memory 'leak'",
        same_stderr=True,
    ),
    # --- `charter workspace optimize`: read-only unless --apply, and --apply only reversible.
    Scenario(
        name="optimize-reads-every-journal-and-changes-nothing",
        plane="daily",
        python=["workspace", "optimize"],
    ),
    Scenario(
        name="optimize-apply-on-a-tidy-journal-does-nothing",
        plane="daily",
        python=["workspace", "optimize", "alpha", "--apply"],
    ),
    Scenario(
        name="optimize-names-what-apply-would-do-and-does-none-of-it",
        plane="daily",
        setup=_duplicates_to_curate,
        python=["workspace", "optimize", "alpha"],
    ),
    Scenario(
        name="optimize-apply-archives-exact-duplicates-and-repairs-the-index",
        plane="daily",
        setup=_duplicates_to_curate,
        python=["workspace", "optimize", "alpha", "--apply"],
    ),
    Scenario(
        name="optimize-apply-numbers-an-archive-name-that-is-taken",
        plane="daily",
        setup=_archive_names_taken,
        python=["workspace", "optimize", "alpha", "--apply"],
    ),
    Scenario(
        name="optimize-apply-does-not-append-through-an-index-linked-out",
        plane="daily",
        setup=_journal_index_linked_out,
        python=["workspace", "optimize", "alpha", "--apply"],
        stderr_mask=[ABSOLUTE_PATHS],
    ),
    Scenario(
        name="optimize-apply-does-not-move-into-an-archive-linked-out",
        plane="daily",
        setup=_journal_archive_linked_out,
        python=["workspace", "optimize", "alpha", "--apply"],
    ),
    Scenario(
        name="optimize-proposes-what-is-older-than-stale-days",
        plane="daily",
        python=["workspace", "optimize", "--stale-days", "10"],
    ),
    Scenario(
        name="optimize-a-workspace-that-is-not-there",
        plane="daily",
        python=["workspace", "optimize", "nope"],
        refusal="no workspace 'nope'",
        same_stderr=True,
    ),
    Scenario(
        name="optimize-names-a-journal-it-could-not-read",
        plane="daily",
        setup=_journal_unreadable,
        python=["workspace", "optimize", "alpha"],
        refusal="cannot be checked",
        same_stderr=True,
    ),
    Scenario(
        name="optimize-on-a-plane-with-no-workspaces",
        plane="minimal",
        python=["workspace", "optimize"],
    ),
    # --- `charter persona recall | remember`.
    Scenario(
        name="persona-recall-lists-its-memory-the-shared-store-and-this-sessions-activity",
        plane="daily",
        python=["persona", "recall", "devops"],
        pins_the_clock=False,
    ),
    Scenario(
        name="persona-recall-a-persona-with-no-memory-of-its-own",
        plane="daily",
        python=["persona", "recall", "steward"],
        pins_the_clock=False,
    ),
    Scenario(
        name="persona-recall-a-query",
        plane="daily",
        python=["persona", "recall", "devops", "-q", "cluster"],
        pins_the_clock=False,
    ),
    Scenario(
        name="persona-recall-a-query-capped-by-log",
        plane="daily",
        python=["persona", "recall", "devops", "-q", "plane", "--log", "1"],
        pins_the_clock=False,
    ),
    Scenario(
        name="persona-recall-a-query-that-matches-nothing",
        plane="daily",
        python=["persona", "recall", "devops", "-q", "zzz"],
        pins_the_clock=False,
    ),
    Scenario(
        name="persona-recall-with-ephemeral-scratch",
        plane="daily",
        setup=_ephemeral_scratch,
        python=["persona", "recall", "devops"],
        pins_the_clock=False,
    ),
    Scenario(
        name="persona-recall-refuses-a-persona-the-plane-does-not-define",
        plane="daily",
        python=["persona", "recall", "nope"],
        pins_the_clock=False,
        refusal="no persona 'nope'",
        same_stderr=True,
    ),
    Scenario(
        name="persona-recall-refuses-a-name-outside-the-alphabet",
        plane="daily",
        python=["persona", "recall", "Bad"],
        pins_the_clock=False,
        refusal="invalid persona name",
        same_stderr=True,
    ),
    Scenario(
        name="persona-remember-its-own",
        plane="daily",
        python=["persona", "remember", "devops", "A fact about devops", "--no-sync"],
    ),
    Scenario(
        name="persona-remember-into-the-shared-store",
        plane="daily",
        python=["persona", "remember", "devops", "Second fact", "--shared", "--no-sync"],
    ),
    Scenario(
        name="persona-remember-into-a-store-with-no-index-yet",
        plane="daily",
        # `steward/memory` holds a `.gitkeep` and no MEMORY.md: charter writes the store's
        # generic `# Memory Index` header, not the persona's.
        python=["persona", "remember", "steward", "A steward fact", "--no-sync"],
    ),
    Scenario(
        name="persona-remember-ephemeral-scratch-is-private",
        plane="daily",
        python=["persona", "remember", "devops", "Scratch fact", "--ephemeral"],
    ),
    Scenario(
        name="persona-remember-under-a-title",
        plane="daily",
        python=["persona", "remember", "devops", "Body text", "--title", "  Short  ",
                "--no-sync"],
    ),
    Scenario(
        name="persona-remember-under-a-title-of-spaces",
        plane="daily",
        python=["persona", "remember", "devops", "Body text", "--title", "   ", "--no-sync"],
    ),
    Scenario(
        name="persona-remember-a-title-already-taken-is-numbered",
        plane="daily",
        python=["persona", "remember", "devops", "Cluster prod-1 lives in eu-west-1",
                "--no-sync"],
    ),
    Scenario(
        name="persona-remember-non-ascii-is-escaped-in-the-trace",
        plane="daily",
        python=["persona", "remember", "devops", "unicode — fact é", "--no-sync"],
    ),
    Scenario(
        name="persona-remember-with-memory-kept-local",
        plane="daily",
        python=["persona", "remember", "devops", "Plain fact"],
    ),
    Scenario(
        name="persona-remember-refuses-an-empty-memory",
        plane="daily",
        python=["persona", "remember", "devops", "  ", "--no-sync"],
        refusal="empty memory",
        same_stderr=True,
    ),
    Scenario(
        name="persona-remember-refuses-a-persona-the-plane-does-not-define",
        plane="daily",
        python=["persona", "remember", "nope", "x", "--no-sync"],
        refusal="no persona 'nope'",
        same_stderr=True,
    ),
    Scenario(
        name="persona-remember-writes-nothing-through-an-index-linked-out",
        plane="daily",
        setup=_persona_index_linked_out,
        python=["persona", "remember", "devops", "A fact", "--no-sync"],
        refusal="outside the directories",
        same_stderr=True,
        stderr_mask=[ABSOLUTE_PATHS],
    ),
    *SAVE_SCENARIOS,
    *WORKTREE_SCENARIOS,
    *DOCS_SCENARIOS,
    *VERSION_SCENARIOS,
]


def _decision(stdout: str) -> str | None:
    """The `permissionDecisionReason` a `PreToolUse` hook printed, or `None`.

    Read rather than string-matched, so a scenario's `denies` is about the FIELD the harness
    acts on and not about a sentence that happens to be somewhere in the output. A hook that
    printed something other than a deny — a `systemMessage`, an allow — answers `None`, which
    is the same failure as printing nothing.
    """
    if not stdout.strip():
        return None
    try:
        out = json.loads(stdout)["hookSpecificOutput"]
    except (ValueError, KeyError, TypeError):
        return None
    if out.get("permissionDecision") != "deny":
        return None
    return out.get("permissionDecisionReason")


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
        # The side's own `bin/` first: where a repo scenario puts its recorded `gh`. Absent
        # for every other scenario, which is the same PATH as before.
        "PATH": f"{root.parent / 'bin'}:/usr/bin:/bin",
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
    if not plane:
        # A directory that is not a plane yet: what `charter init` is pointed at.
        root.mkdir()
        return root, home, pins
    shutil.copytree(PLANES / plane, root)
    # The fixtures cannot carry an empty directory, so the generator records them beside the
    # plane. Restoring them matters: "no `todos/`" and "an empty `todos/`" are different
    # starting states, and charter's readers answer differently for each.
    listing = PLANES / f"{plane}.empty-dirs"
    if listing.exists():
        for rel in listing.read_text().split():
            (root / rel).mkdir(parents=True, exist_ok=True)
    return root, home, pins


def _run(argv: list[str], root: Path, home: Path, pins: Path, stdin: str = "",
         extra: "dict[str, str | None] | None" = None,
         cwd: str = "") -> subprocess.CompletedProcess:
    env = _env(root, home, pins)
    for name, value in (extra or {}).items():
        if value is None:
            env.pop(name, None)
        else:
            env[name] = value
    # `input=` always, never an inherited descriptor: a command that reads stdin would
    # otherwise get this harness's, which is a terminal when `./run.py` is typed and a closed
    # pipe in CI. `""` is a clean EOF on both.
    #
    # **It decides a resolution rung as well as a payload**, which is why no scenario may go
    # back to inheriting one: `session.terminal()`'s last rung is `os.ttyname(0)`, so with a
    # terminal on stdin Python charter has a per-terminal pointer id and the answer to "which
    # workspace" would depend on where the suite was run. A pipe is not a terminal, so that
    # rung is OFF unless a scenario turns it on with `env={"TERM_SESSION_ID": …}`.
    return subprocess.run(
        argv, cwd=root / cwd if cwd else root, env=env, capture_output=True, text=True,
        input=stdin,
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


def _planted(scenario: Scenario, root: Path) -> set[str]:
    """Which of `never_says`' needles this scenario actually puts in front of the command.

    Looked for where a command can read it: the brief on stdin, and the bytes of the plane
    copy as it stands after `setup` and before the command runs. A needle found in neither
    is one no implementation could print, which is what makes asserting its absence
    meaningless — so the caller reports it rather than passing.
    """
    found = {needle for needle in scenario.never_says if needle in scenario.stdin}
    outstanding = [n for n in scenario.never_says if n not in found]
    if not outstanding:
        return found
    for path in root.rglob("*"):
        if not path.is_file() or path.is_symlink():
            continue
        try:
            text = path.read_text()
        except (OSError, UnicodeDecodeError):
            continue
        found |= {needle for needle in outstanding if needle in text}
    return found


def _never_said(scenario: Scenario, planted: set[str],
                py: subprocess.CompletedProcess,
                rs: subprocess.CompletedProcess) -> list[str]:
    """Neither side printed a needle — and each needle was really there to print."""
    problems = []
    for needle in scenario.never_says:
        if needle not in planted:
            problems.append(
                f"    never_says {needle!r} is not in this scenario's stdin and not in its "
                "plane, so nothing could have printed it — the assertion is vacuous"
            )
            continue
        for side, run in (("python", py), ("rust", rs)):
            for stream, text in (("stdout", run.stdout), ("stderr", run.stderr)):
                if needle in text:
                    problems.append(
                        f"    {side} PRINTED THE SECRET on {stream}: {needle!r} — a "
                        "credential is refused by KIND, never by the matched text"
                    )
    return problems


def _declared(scenario: Scenario, py: subprocess.CompletedProcess,
              rs: subprocess.CompletedProcess, py_root: Path, rs_root: Path) -> list[str]:
    """Check a `Divergence` in full — see its docstring for why each clause is here."""
    d = scenario.diverges
    assert d is not None
    problems: list[str] = []
    if not (re.search(r"\bADR \d{4}\b", d.why) and re.search(r"\bdecision \d+\b", d.why)):
        problems.append(
            "    this divergence's `why` names no record — it must cite the ADR that decided "
            "it and the spec decision that carries it, or the next reader has a difference "
            "with no author"
        )
    if d.python_exit == d.rust_exit and not d.python_writes and not d.rust_writes:
        problems.append(
            "    this divergence declares the same exit status on both sides and no path "
            "either side writes alone, so it asserts no difference at all"
        )
    for side, got, want in (("python", py.returncode, d.python_exit),
                            ("rust", rs.returncode, d.rust_exit)):
        if got != want:
            problems.append(
                f"    {side} exited {got}, and this divergence says it exits {want} — the "
                f"declared difference is not the one that happened ({d.why})"
            )
    said = _masked(rs.stderr, scenario)
    if said != d.rust_stderr:
        problems.append("    rust's stderr is not what this divergence declares:")
        problems.extend(
            f"      {line}" for line in difflib.unified_diff(
                d.rust_stderr.splitlines(), said.splitlines(), "declared", "rust",
                lineterm="", n=2)
        )
    if not d.python_stderr_has:
        problems.append(
            "    this divergence pins nothing the ORACLE says, so it cannot see charter "
            "changing — set python_stderr_has to the line this difference is a difference from"
        )
    elif d.python_stderr_has not in py.stderr:
        problems.append(
            f"    python no longer says {d.python_stderr_has!r}, which is what this "
            f"divergence is a divergence FROM — it says {py.stderr.strip()!r}"
        )
    for side, root, other, others_root in (
        ("python", py_root, "rust", rs_root),
        ("rust", rs_root, "python", py_root),
    ):
        for rel in (d.python_writes if side == "python" else d.rust_writes):
            # `lexists`: a link is a thing one side wrote, whatever it points at.
            if not os.path.lexists(root / rel):
                problems.append(
                    f"    {side} no longer writes {rel!r}, which this divergence says is "
                    f"{side}'s alone — drop it, or find out what changed"
                )
            if os.path.lexists(others_root / rel):
                problems.append(
                    f"    {other} now writes {rel!r} too, so the declared divergence is "
                    f"GONE for that path — the two implementations agree again ({d.why})"
                )
    return problems


def _masked(text: str, scenario: Scenario) -> str:
    """*text* with each of the scenario's STDERR masks blanked — what neither side's words
    decide."""
    return _mask_with(text, scenario.stderr_mask)


def _mask_with(text: str, masks: list) -> str:
    for pattern, _why in masks:
        text = re.sub(pattern, "<masked>", text)
    return text


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
        # Before the command, because a needle may be in a file the command is about to
        # refuse over and a later look would find the plane already changed.
        planted = _planted(scenario, py_root) if scenario.never_says else set()
        # BEFORE the command: `git-policy --apply` writes git config, and what is being asked
        # is what charter is about to read, not what it left behind.
        trap = (_forge_trap(py_root, "python", scenario)
                + _forge_trap(rs_root, "rust", scenario))

        py = _run([sys.executable, "-m", "charter", *scenario.python], py_root, py_home, py_pins,
                  scenario.stdin, scenario.env, scenario.cwd)
        rust_argv = [str(binary), *scenario.rust_args()]
        if scenario.pins_the_clock:
            rust_argv += ["--now", NOW_NAIVE]
        rs = _run(rust_argv, rs_root, rs_home, rs_pins, scenario.stdin, scenario.env,
                  scenario.cwd)

        problems: list[str] = list(trap)
        # First, and on the RAW output: every note below rewrites a stream — a mask, a
        # `rust_only_lines` deletion, a cut — and a secret is not less printed for having
        # been deleted from the copy this suite compares.
        if scenario.never_says:
            problems.extend(_never_said(scenario, planted, py, rs))
        if scenario.python_only_items:
            text, stale = _python_only(py.stderr, scenario.python_only_items)
            for pattern in stale:
                problems.append(
                    f"    python no longer lists {pattern!r}, but the scenario still says it is "
                    "python's alone — drop the note"
                )
            py = subprocess.CompletedProcess(py.args, py.returncode, py.stdout, text)
        if scenario.rust_only_lines:
            text, block, stale = _rust_only(rs.stderr, scenario.rust_only_lines)
            for pattern in stale:
                problems.append(
                    f"    rust no longer prints {pattern!r}, but the scenario still says it is "
                    "rust's alone — drop the note"
                )
            if not scenario.rust_only_block:
                problems.append(
                    "    this scenario deletes lines from rust's stderr and says nothing about "
                    "what they said — set rust_only_block to the block it takes out, or the "
                    "differential is not looking at it at all"
                )
            else:
                said = _masked(block, scenario)
                if said != scenario.rust_only_block:
                    problems.append("    the block rust prints and charter does not differs:")
                    problems.extend(
                        f"      {line}" for line in difflib.unified_diff(
                            scenario.rust_only_block.splitlines(),
                            said.splitlines(), "declared", "rust", lineterm="", n=2)
                    )
            rs = subprocess.CompletedProcess(rs.args, rs.returncode, rs.stdout, text)
        elif scenario.rust_only_block:
            problems.append(
                "    rust_only_block is set but no rust_only_lines pattern takes any line out, "
                "so the block it declares is compared against nothing"
            )
        if scenario.diverges is not None:
            # A DECIDED difference states its own exit statuses and its own stderr, and they
            # are checked INSTEAD of "the two must match" — never as well, because the two
            # cannot match and that is the point. Everything else about the scenario, the
            # tree comparison below included, still applies unchanged.
            problems.extend(_declared(scenario, py, rs, py_root, rs_root))
        else:
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
                if scenario.same_stderr:
                    want, got = _masked(py.stderr, scenario), _masked(rs.stderr, scenario)
                    if want != got:
                        problems.append("    stderr differs:")
                        problems.append(f"      python {want!r}")
                        problems.append(f"      rust   {got!r}")
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
                elif scenario.stderr_cut_at:
                    if scenario.stderr_cut_at not in py.stderr:
                        problems.append(
                            f"    python's stderr never reaches {scenario.stderr_cut_at!r}, so "
                            "this scenario is cutting at a boundary that no longer exists"
                        )
                    else:
                        want = py.stderr.split(scenario.stderr_cut_at, 1)[0]
                        got = rs.stderr
                        for pattern, _why in scenario.stderr_mask:
                            want = re.sub(pattern, "<masked>", want)
                            got = re.sub(pattern, "<masked>", got)
                        if want != got:
                            problems.append(
                                f"    stderr differs before {scenario.stderr_cut_at!r}:"
                            )
                            problems.append(f"      python {want!r}")
                            problems.append(f"      rust   {got!r}")
                elif _masked(py.stderr, scenario) != _masked(rs.stderr, scenario):
                    problems.append("    stderr differs:")
                    problems.append(f"      python {py.stderr!r}")
                    problems.append(f"      rust   {rs.stderr!r}")
        one_sided = scenario.diverges.one_sided() if scenario.diverges else {}
        problems.extend(_diff_trees(py_root, rs_root, {**scenario.ignore, **one_sided}))
        if scenario.facts is not None:
            py_facts, rs_facts = scenario.facts(py_root), scenario.facts(rs_root)
            if py_facts != rs_facts:
                problems.append("    facts differ:")
                problems.extend(
                    f"      {line}" for line in difflib.unified_diff(
                        py_facts.splitlines(), rs_facts.splitlines(), "python", "rust",
                        lineterm="", n=1)
                )
        escapes = _escaped("python", py_before, _outside(scratch, "python", py_root)) + _escaped(
            "rust", rs_before, _outside(scratch, "rust", rs_root)
        )
        for prefix in scenario.writes_outside:
            if not any(f": {prefix}" in line for line in escapes):
                problems.append(
                    f"    neither side wrote {prefix!r}, but the scenario says that is where "
                    "this command's work lands — drop the note, or find out why nothing was "
                    "pushed"
                )

        def allowed(line: str) -> bool:
            if any(f": {prefix}" in line for prefix in scenario.writes_outside):
                return True  # either side; this is where the command's work lands
            # Python's alone: the Rust side writing one of these is still an escape.
            return line.startswith("    python ") and any(
                f": {path} (" in line for path in scenario.python_writes_outside
            )

        problems.extend(line for line in escapes if not allowed(line))
        if scenario.stdout_differs:
            if py.stdout == rs.stdout:
                problems.append(
                    "    stdout now MATCHES, but the scenario still says it differs "
                    f"({scenario.stdout_differs}) — drop the note"
                )
        elif scenario.stdout_cut_at:
            cut = scenario.stdout_cut_at
            missing = [side for side, out in (("python", py.stdout), ("rust", rs.stdout))
                       if cut not in out]
            if missing:
                problems.append(
                    f"    {' and '.join(missing)} never reach {cut!r} on stdout, so this "
                    "scenario is cutting at a boundary that is not in both renders"
                )
            else:
                want, got = py.stdout.split(cut, 1)[0], rs.stdout.split(cut, 1)[0]
                if not want:
                    # The second half of the fence. Reaching the boundary is not enough on its
                    # own: a marker that turned out to be the first thing charter prints would
                    # leave two empty prefixes, and a comparison of nothing against nothing
                    # reports `ok` for every implementation there could be.
                    problems.append(
                        f"    python prints nothing before {cut!r}, so this scenario compares "
                        "two empty strings and asserts nothing"
                    )
                elif want != got:
                    problems.append(f"    stdout differs before {cut!r}:")
                    problems.append(f"      python {want!r}")
                    problems.append(f"      rust   {got!r}")
        elif scenario.stdout_rewrite:
            want, trouble = _rewritten_stdout(scenario, py.stdout, rs.stdout)
            problems.extend(trouble)
            if want != rs.stdout:
                problems.append(
                    "    stdout differs BEYOND what this scenario's rewrites declare:"
                )
                problems.extend(
                    f"      {line}" for line in difflib.unified_diff(
                        want.splitlines(), rs.stdout.splitlines(),
                        "python, rewritten as declared", "rust", lineterm="", n=1)
                )
        else:
            # `stdout_mask` is empty for every scenario that does not set it, so this is the
            # plain comparison for all of them and a masked one only where one is declared.
            want = _mask_with(py.stdout, scenario.stdout_mask)
            got = _mask_with(rs.stdout, scenario.stdout_mask)
            if want != got:
                problems.append("    stdout differs:")
                problems.append(f"      python {want!r}")
                problems.append(f"      rust   {got!r}")

        # A hook's verdict is on STDOUT, so the check above — "the two match" — is satisfied by
        # two sides that both allowed. These two say WHICH, on each side separately, so a guard
        # that stopped firing is red rather than symmetrically silent.
        if scenario.denies:
            for side, out in (("python", py.stdout), ("rust", rs.stdout)):
                said = _decision(out)
                if said is None:
                    problems.append(
                        f"    {side} printed no PreToolUse denial at all: {out!r}"
                    )
                elif scenario.denies not in said:
                    problems.append(
                        f"    {side} denied with something else — wanted {scenario.denies!r}, "
                        f"got {said!r}"
                    )
        if scenario.allows:
            if scenario.denies:
                problems.append(
                    "    this scenario says both `allows` and `denies`, which cannot both be "
                    "what the hook answered"
                )
            for side, out in (("python", py.stdout), ("rust", rs.stdout)):
                if out:
                    problems.append(f"    {side} did not allow — it printed {out!r}")

        if scenario.stdout_rewrite and (scenario.stdout_differs or scenario.stdout_cut_at):
            # Both of those take precedence in the chain above, so the rewrites would be
            # declaring a difference that nothing compares.
            problems.append(
                "    stdout_rewrite is set beside stdout_differs or stdout_cut_at, and those "
                "answer first — so these rewrites assert nothing"
            )

        if scenario.alerts is not None:
            problems.extend(_alert_rows(scenario, py.stdout, rs.stdout))
        elif scenario.alert_rewrite is not None:
            problems.append(
                "    alert_rewrite is set but the scenario compares no alert rows — set alerts"
            )

        print(("ok   " if not problems else "DIFF ") + scenario.name)
        for line in problems:
            print(line)
        return not problems


#: What makes a status-line row an ALERT row: the `⚠` every `_alerts` row opens with, reset
#: straight after — `{accent}⚠{_R} `. Nothing else charter draws has that shape: the identity
#: row's reinit tip is `⚠ reinit:` and the session strip's cold cache is `⚠ cache cold`, both
#: with the glyph and its words in ONE colour, so neither is picked up.
ALERT_MARK = "⚠\x1b[0m "


def _alert_lines(stdout: str) -> list[str]:
    return [line for line in stdout.splitlines() if ALERT_MARK in line]


def _rewritten_stdout(scenario: Scenario, py_out: str, rs_out: str) -> "tuple[str, list[str]]":
    """charter's stdout with charter-app's words in it — see `Scenario.stdout_rewrite`.

    Both halves are held to being there. Without the first check a rewrite would go on
    "allowing" a difference charter had stopped making; without the second it would allow one
    charter-app had stopped making, which is the same rot in the other direction.
    """
    problems = []
    text = py_out
    for theirs, ours, why in scenario.stdout_rewrite:
        if theirs not in text:
            problems.append(
                f"    python no longer prints what a rewrite replaces, so it is a rewrite of "
                f"nothing: {_opening(theirs)!r} ({why})"
            )
            continue
        if ours not in rs_out:
            problems.append(
                f"    rust does not print what a rewrite replaces it with, so the declaration "
                f"has outlived the difference: {_opening(ours)!r} ({why})"
            )
        text = text.replace(theirs, ours, 1)
    return text, problems


def _rewritten(line: str, theirs: str, ours: str) -> str:
    """*line* with charter's words replaced by charter-app's, **and the frame kept true**.

    A row is boxed: after its last reset comes the fill that carries it out to the right border.
    Replacing words of a different length and leaving the fill alone would move that border, and
    the comparison would then report the difference it was told to allow as a different one. So
    the fill is moved by exactly the length the words changed by, right after the reset that
    closes them — both literals are ASCII, so length is width.
    """
    at = line.find(theirs)
    if at < 0:
        return line
    rest = line[at + len(theirs):]
    reset = rest.find("\x1b[0m")
    if reset < 0:
        return line
    reset += len("\x1b[0m")
    # Only ever shorter: a longer replacement would have to take fill that may not be there.
    assert len(ours) <= len(theirs), "an alert_rewrite may only shorten charter's words"
    return line[:at] + ours + rest[:reset] + " " * (len(theirs) - len(ours)) + rest[reset:]


def _alert_rows(scenario: Scenario, py_out: str, rs_out: str) -> list[str]:
    """Compare the alert rows of both status lines — see `Scenario.alerts`."""
    problems: list[str] = []
    want, got = _alert_lines(py_out), _alert_lines(rs_out)
    if len(want) != scenario.alerts:
        problems.append(
            f"    charter draws {len(want)} alert row(s) and this scenario says "
            f"{scenario.alerts} — the plane it sets up no longer produces what it is named for:"
        )
        problems.extend(f"      python {line!r}" for line in want)
    if scenario.alert_rewrite is not None:
        theirs, ours, why = scenario.alert_rewrite
        if not re.search(r"\bADR \d{4}\b", why):
            problems.append("    this alert_rewrite's `why` names no ADR")
        if not any(theirs in line for line in want):
            problems.append(
                f"    charter no longer says {theirs!r} in an alert row, which is what this "
                "rewrite is a difference FROM — drop it, or find out what changed"
            )
        if not any(ours in line for line in got):
            problems.append(
                f"    charter-app no longer says {ours!r} in an alert row, which is what this "
                "rewrite says it says instead"
            )
        want = [_rewritten(line, theirs, ours) for line in want]
    if want != got:
        problems.append("    alert rows differ:")
        problems.extend(
            f"      {line}" for line in difflib.unified_diff(
                [repr(x) for x in want], [repr(x) for x in got], "python", "rust",
                lineterm="", n=1)
        )
    return problems


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--binary", type=Path, default=REPO / "target" / "debug" / "charter",
                    help="the Rust charter to test (default: target/debug/charter)")
    ap.add_argument("--scenario", action="append", default=None,
                    help="run only this scenario (repeatable)")
    ap.add_argument("--time-preflight", type=int, default=0, metavar="RUNS",
                    help="after the scenarios, time `charter doctor --preflight` on both sides")
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

    # `charter doctor`'s scenarios compare a document rather than a plane write, so they live
    # beside this file with their own comparison (`doctor_scenarios.py`).
    import doctor_scenarios

    everything = [*SCENARIOS, *doctor_scenarios.DOCTOR_SCENARIOS]
    wanted = everything
    if args.scenario:
        names = {s.name for s in everything}
        # An exact name, or a PREFIX of one. The prefix is what makes a family runnable while
        # it is being written — `--scenario pretooluse` is 25 scenarios — and it cannot
        # silently select nothing, because a word that matches no name is still an error.
        unknown = [n for n in args.scenario
                   if n not in names and not any(m.startswith(n) for m in names)]
        if unknown:
            ap.error(f"no such scenario: {', '.join(unknown)} (have {', '.join(sorted(names))})")
        wanted = [s for s in everything
                  if any(s.name == n or s.name.startswith(n) for n in args.scenario)]

    # Before the scenarios, and whichever of them were asked for: a corpus that has drifted makes
    # every `news --for` scenario report a difference in a rendered body, and this names the file.
    drifted = not check_corpus()
    # The same, for the pages `docs show` prints: a scenario generated from the vendored
    # directory cannot see a page that is no longer in it.
    docs_drifted = not check_docs_corpus()

    failed = [
        s.name for s in wanted
        if not (doctor_scenarios.check(s, args.binary)
                if isinstance(s, doctor_scenarios.DoctorScenario) else check(s, args.binary))
    ]
    if drifted:
        failed.append("news-corpus")
    if docs_drifted:
        failed.append("docs-corpus")
    if args.time_preflight:
        print()
        doctor_scenarios.time_preflight(args.binary, args.time_preflight)
    print()
    if failed:
        print(f"{len(failed)} of {len(wanted)} scenarios differ: {', '.join(failed)}")
        return 1
    print(f"all {len(wanted)} scenarios identical")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
