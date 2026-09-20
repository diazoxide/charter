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
import difflib
import filecmp
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
    #: What each side's plane must agree on that is not a file the tree comparison can read
    #: byte for byte — a clone's branch, commit and config, which live under a `.git` whose
    #: index and reflogs carry timestamps and inode numbers. Run against each plane after the
    #: command; the two answers must be equal. The `.git` itself is then `ignore`d, and this
    #: is what stands in for it rather than nothing.
    facts: "Callable[[Path], str] | None" = None

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

SCENARIOS = [
    *INIT_SCENARIOS,
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
        if scenario.python_only_items:
            text, stale = _python_only(py.stderr, scenario.python_only_items)
            for pattern in stale:
                problems.append(
                    f"    python no longer lists {pattern!r}, but the scenario still says it is "
                    "python's alone — drop the note"
                )
            py = subprocess.CompletedProcess(py.args, py.returncode, py.stdout, text)
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
            elif py.stderr != rs.stderr:
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
        problems.extend(
            line for line in _escaped("python", py_before, _outside(scratch, "python", py_root))
            if not any(f": {path} (" in line for path in scenario.python_writes_outside)
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
