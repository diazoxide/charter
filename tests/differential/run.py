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


def _both(*steps):
    def setup(root: Path) -> None:
        for step in steps:
            step(root)
    return setup


INIT = ["init", "--forge", "github", "--owner", "acme"]


def _init(name: str, *, python=INIT, plane="", setup=None, refusal="") -> Scenario:
    """An `init` or `reinit` scenario: no clock, opencode's global files installed first, and
    the whole tree each side leaves compared."""
    return Scenario(
        name=name,
        plane=plane,
        python=python,
        pins_the_clock=False,
        setup=_both(_opencode_already_installed, *([setup] if setup else [])),
        refusal=refusal,
        python_only_items=[] if refusal or python[0] == "reinit" else [OPENCODE_SHIM],
        python_writes_outside={OPENCODE_CONTEXT: OPENCODE_CONTEXT_WHY},
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
    for topic in sorted(set(theirs) & set(ours)):
        if theirs[topic] != ours[topic]:
            problems.append(f"    differs: {topic}.md")
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


SCENARIOS = [
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


def _masked(text: str, scenario: Scenario) -> str:
    """*text* with each of the scenario's masks blanked — what neither side's words decide."""
    for pattern, _why in scenario.stderr_mask:
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
        problems.extend(_diff_trees(py_root, rs_root, scenario.ignore))
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
        unknown = [n for n in args.scenario if n not in names]
        if unknown:
            ap.error(f"no such scenario: {', '.join(unknown)} (have {', '.join(sorted(names))})")
        wanted = [s for s in everything if s.name in set(args.scenario)]

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
