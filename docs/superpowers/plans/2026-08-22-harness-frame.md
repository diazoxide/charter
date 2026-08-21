# Harness Frame Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `charter claude` (and `codex`, `opencode`, `frame -- <cmd>`) runs the harness inside a tmux-composed frame with charter panels on the edges, giving every harness the surface only Claude Code's status line provides today.

**Architecture:** tmux composes the rectangles and owns all terminal emulation; charter fills them. Each panel is a charter process that owns its pane, drawing with `charter/tui.py` primitives and waking on a version file the existing hooks bump. Charter never parses or draws the harness's pane.

**Tech Stack:** Python 3.11+, stdlib only. tmux ≥ 3.2 as an external binary. Tests are stdlib `unittest`, run with `python -m unittest discover -s tests -v`.

**Spec:** `docs/superpowers/specs/2026-08-21-harness-wrapper-design.md`

## Global Constraints

- **Zero runtime dependencies.** `pyproject.toml` ships `dependencies = []`. Nothing in this feature may add one.
- **tmux floor is 3.2** — `display-menu` needs 3.0, `display-popup` needs 3.2. Probe `tmux -V`; below the floor, start the frame with the hotkey disabled and say which feature needs which version.
- **Never join argv.** Pinned against tmux 3.7c: separate arguments are not shell-interpreted, a joined string is. Every tmux invocation passes a list; no `" ".join` anywhere in `charter/frame/`.
- **Names never enter a tmux command string.** Workspace, repo, branch and persona names come from committed files and `.git/HEAD`. Menu items carry opaque ids resolved in-process.
- **`_PANE_ID_VARS` is not reordered.** The frame mints its own id; existing sessions keep their behaviour.
- **`CHARTER_WORKSPACE` is never exported** by the frame — `statusline._active()` prefers it over the pointer file, so exporting it would make `charter ws use` inside the frame appear to do nothing.
- **The status line path is untouched.** No edits to `charter/statusline.py`'s render path; slots *call* its helpers.
- **Tests never require tmux.** Anything needing the binary skips when it is absent, and asserts on probed capability rather than a version string.

---

### Task 1: A harness knows how to start itself

**Files:**
- Modify: `charter/harness/base.py` (add two members to `Harness`)
- Modify: `charter/harness/claude_code.py`, `charter/harness/codex.py`, `charter/harness/opencode.py`
- Test: `tests/test_harness_launch.py`

**Interfaces:**
- Consumes: `charter.harness.registry.KINDS`, `charter.harness.base.Harness`
- Produces: `Harness.cli_name: str` (the word after `charter`), `Harness.launch_argv(extra: list[str]) -> list[str]`

- [ ] **Step 1: Write the failing test**

```python
"""A harness that charter can run has to say what to type and what to exec.

`registry.KINDS` exists so a harness added to it is covered everywhere the day it is
registered. That only holds if the launcher reads these two facts off the harness rather
than keeping its own table — the hardcoded-literal problem the registry was built to end.
"""

from __future__ import annotations

import unittest

from charter import cli, harness


class HarnessLaunchIdentity(unittest.TestCase):
    def test_every_registered_harness_says_what_to_type(self):
        for h in harness.all():
            with self.subTest(harness=h.name):
                self.assertTrue(h.cli_name, f"{h.name} has no cli_name")

    def test_cli_names_are_distinct(self):
        names = [h.cli_name for h in harness.all()]
        self.assertEqual(len(names), len(set(names)), f"colliding cli_names: {names}")

    def test_a_cli_name_never_shadows_a_core_command(self):
        """A harness called `status` would take `charter status` from the operator.
        Failing here is the point: the collision is caught in CI, not in a terminal."""
        parser = cli.build_parser()
        core = set()
        for action in parser._subparsers._group_actions:
            core.update(action.choices)
        for h in harness.all():
            with self.subTest(harness=h.name):
                self.assertNotIn(h.cli_name, core - {h.cli_name},
                                 f"{h.name}'s cli_name shadows a core command")

    def test_launch_argv_passes_the_operators_arguments_through_verbatim(self):
        h = harness.get(harness.CLAUDE_CODE)
        self.assertEqual(h.launch_argv(["--resume", "a;b"]),
                         ["claude", "--resume", "a;b"])

    def test_launch_argv_returns_a_list_never_a_string(self):
        """Pinned against tmux 3.7c: separate argv is not shell-interpreted, a joined
        string is. A harness returning a string would put the injection back."""
        for h in harness.all():
            with self.subTest(harness=h.name):
                self.assertIsInstance(h.launch_argv([]), list)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_harness_launch -v`
Expected: FAIL — `AttributeError: 'ClaudeCodeHarness' object has no attribute 'cli_name'`

- [ ] **Step 3: Add the two members to the base class**

In `charter/harness/base.py`, inside `class Harness`, after `deficits`:

```python
    #: The word an operator types after ``charter`` to run this harness in a frame.
    #: Distinct from :attr:`name`, which is the harness's own identity in
    #: ``$CHARTER_HARNESS``: ``claude-code`` names the harness, ``claude`` is the binary
    #: and what a hand types. Empty means charter cannot launch this harness.
    cli_name: str = ""

    #: The binary to exec. Separate from :attr:`cli_name` because they differ.
    binary: str = ""

    def launch_argv(self, extra: list[str]) -> list[str]:
        """Argv for starting this harness, with the operator's arguments appended.

        A **list**, never a joined string, and that is a security property rather than a
        style preference: tmux does not shell-interpret separate arguments and does
        interpret a joined one (pinned against 3.7c). Returning a string here would put
        command injection back into every launch.
        """
        return [self.binary, *extra]
```

- [ ] **Step 4: Give each harness its two values**

`charter/harness/claude_code.py`, in `class ClaudeCodeHarness` after `deficits = ()`:

```python
    cli_name = "claude"
    binary = "claude"
```

`charter/harness/codex.py`, in `class CodexHarness`:

```python
    cli_name = "codex"
    binary = "codex"
```

`charter/harness/opencode.py`, in `class OpenCodeHarness`:

```python
    cli_name = "opencode"
    binary = "opencode"
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `python -m unittest tests.test_harness_launch -v`
Expected: PASS, 5 tests

- [ ] **Step 6: Run the whole suite for regressions**

Run: `python -m unittest discover -s tests -v 2>&1 | tail -5`
Expected: OK

- [ ] **Step 7: Commit**

```bash
git add charter/harness/ tests/test_harness_launch.py
git commit -m "A harness says what to type and what to exec (#345)"
```

---

### Task 2: The `[frame]` config section

**Files:**
- Modify: `charter/instance.py` (add `frame_of`)
- Modify: `charter/config.py:derive` (add `d["FRAME"]`)
- Test: `tests/test_frame_config.py`

**Interfaces:**
- Consumes: `charter.instance.load`
- Produces: `instance.frame_of(cfg: dict) -> dict` and `config.FRAME` with keys `slots: list[str]`, `mouse: bool`, `hotkey: str`, `history_limit: int`, `min_cols: int`, `min_rows: int`

- [ ] **Step 1: Write the failing test**

```python
"""`[frame]` in charter.toml, with defaults that hold when it is absent.

Defaults are the shipped behaviour, so they are asserted rather than assumed: `mouse` is
off because `set -g mouse on` takes over drag-select, and breaking the operator's copy to
enable a feature v1 does not ship is a bad trade.
"""

from __future__ import annotations

import unittest

from charter import instance


class FrameDefaults(unittest.TestCase):
    def test_an_absent_section_yields_the_shipped_defaults(self):
        f = instance.frame_of({})
        self.assertEqual(f["slots"], ["top", "bottom"])
        self.assertIs(f["mouse"], False)
        self.assertEqual(f["hotkey"], "F2")
        self.assertEqual(f["history_limit"], 50000)
        self.assertEqual(f["min_cols"], 100)
        self.assertEqual(f["min_rows"], 20)

    def test_a_section_overrides_only_what_it_names(self):
        f = instance.frame_of({"frame": {"mouse": True, "hotkey": "F5"}})
        self.assertIs(f["mouse"], True)
        self.assertEqual(f["hotkey"], "F5")
        self.assertEqual(f["slots"], ["top", "bottom"])

    def test_an_unknown_slot_is_dropped_rather_than_carried(self):
        """A typo must not reach a tmux argv. Dropping is louder than it looks: the slot
        simply does not appear, and `doctor` has the config to report."""
        f = instance.frame_of({"frame": {"slots": ["top", "sideways", "bottom"]}})
        self.assertEqual(f["slots"], ["top", "bottom"])

    def test_a_malformed_section_falls_back_instead_of_raising(self):
        """`config` is imported by every command including `charter --version`, so a bad
        value must never crash import."""
        f = instance.frame_of({"frame": {"slots": "top", "history_limit": "lots"}})
        self.assertEqual(f["slots"], ["top", "bottom"])
        self.assertEqual(f["history_limit"], 50000)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_config -v`
Expected: FAIL — `AttributeError: module 'charter.instance' has no attribute 'frame_of'`

- [ ] **Step 3: Implement `frame_of`**

Append to `charter/instance.py`:

```python
#: The edges a frame may occupy. A slot outside this set is a typo, and a typo must not
#: reach a tmux argv — so an unknown one is dropped rather than passed through.
FRAME_SLOTS = ("top", "bottom", "left", "right")

FRAME_DEFAULTS = {
    "slots": ["top", "bottom"],
    "mouse": False,
    "hotkey": "F2",
    "history_limit": 50000,
    "min_cols": 100,
    "min_rows": 20,
}


def frame_of(cfg: dict) -> dict:
    """The ``[frame]`` section merged over :data:`FRAME_DEFAULTS`.

    Every value is type-checked against its default and discarded if it disagrees. This
    module is imported by every command, including ``charter --version``, so a
    hand-edited charter.toml must degrade to the defaults rather than raise.
    """
    out = dict(FRAME_DEFAULTS)
    section = cfg.get("frame")
    if not isinstance(section, dict):
        return out
    for key, default in FRAME_DEFAULTS.items():
        if key not in section:
            continue
        value = section[key]
        if key == "slots":
            if isinstance(value, list):
                kept = [s for s in value if s in FRAME_SLOTS]
                if kept:
                    out[key] = kept
        elif isinstance(value, type(default)) and not isinstance(value, bool) ^ isinstance(default, bool):
            out[key] = value
    return out
```

- [ ] **Step 4: Expose it on `config`**

In `charter/config.py`, inside `derive()`, after the `MEMORY_SHARE` line:

```python
    #: How `charter <harness>` composes its frame. Defaults live in
    #: `instance.FRAME_DEFAULTS`; an absent or malformed section yields them whole.
    d["FRAME"] = _instance.frame_of(cfg)
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_config tests.test_config -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add charter/instance.py charter/config.py tests/test_frame_config.py
git commit -m "The [frame] section, with defaults that survive a bad edit (#345)"
```

---

### Task 3: `frame/layout.py` — the frame's shape, as a pure function

**Files:**
- Create: `charter/frame/__init__.py`
- Create: `charter/frame/layout.py`
- Test: `tests/test_frame_layout.py`

**Interfaces:**
- Consumes: nothing (deliberately — this module imports no tmux and no charter state)
- Produces:
  - `visible_slots(slots: list[str], cols: int, rows: int, min_cols: int, min_rows: int) -> list[str]`
  - `Pane = NamedTuple("Pane", [("slot", str), ("argv", list[str]), ("size", int)])`
  - `plan(*, slots, cols, rows, harness_argv, session, conf, socket, charter_argv) -> list[list[str]]`

- [ ] **Step 1: Write the failing test**

```python
"""The frame's whole shape, decided without tmux.

Layout is pure so the feature is testable on a machine that has never installed tmux, and
so the argv rule is enforced by a unit test rather than by review: every element of every
command is a separate string, because tmux shell-interprets a joined one (pinned against
3.7c) and workspace, repo and branch names all reach here from committed files.
"""

from __future__ import annotations

import unittest

from charter.frame import layout


BASE = dict(cols=200, rows=50, harness_argv=["claude", "--resume", "a;b"],
            session="charter-demo-1234", conf="/tmp/f/tmux.conf",
            socket="charter", charter_argv=["charter"])


class VisibleSlots(unittest.TestCase):
    def test_a_wide_tall_terminal_keeps_every_slot(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "left", "right"], 200, 50, 100, 20),
            ["top", "bottom", "left", "right"])

    def test_side_panels_go_first_when_the_terminal_is_narrow(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "left", "right"], 80, 50, 100, 20),
            ["top", "bottom"])

    def test_the_top_goes_next_when_the_terminal_is_short(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "left", "right"], 200, 12, 100, 20),
            ["bottom"])

    def test_a_tiny_terminal_keeps_nothing(self):
        """Below the floor the harness gets the whole terminal. Degrading to a bare
        harness is the same move `statusline.render` makes when it runs out of width."""
        self.assertEqual(layout.visible_slots(["top", "bottom"], 40, 8, 100, 20), [])


class Plan(unittest.TestCase):
    def test_the_harness_argv_survives_as_separate_elements(self):
        cmds = layout.plan(slots=[], **BASE)
        joined = [c for c in cmds if any("claude --resume" in part for part in c)]
        self.assertEqual(joined, [], "harness argv was joined into one string")
        flat = [part for c in cmds for part in c]
        self.assertIn("--resume", flat)
        self.assertIn("a;b", flat)

    def test_every_command_is_a_list_of_separate_strings(self):
        for cmd in layout.plan(slots=["top", "bottom"], **BASE):
            self.assertIsInstance(cmd, list)
            for part in cmd:
                self.assertIsInstance(part, str)

    def test_each_visible_slot_gets_one_panel_command(self):
        cmds = layout.plan(slots=["top", "bottom"], **BASE)
        panels = [c for c in cmds if "panel" in c]
        self.assertEqual(len(panels), 2)
        slots = {c[c.index("panel") + 1] for c in panels}
        self.assertEqual(slots, {"top", "bottom"})

    def test_the_socket_is_named_on_every_command(self):
        """One private server, never the operator's. Every command carries `-L`."""
        for cmd in layout.plan(slots=["top"], **BASE):
            self.assertEqual(cmd[:3], ["tmux", "-L", "charter"])


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_layout -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'charter.frame'`

- [ ] **Step 3: Create the package and the layout module**

`charter/frame/__init__.py`:

```python
"""charter's own frame: the harness runs inside it, charter draws around it.

tmux composes the rectangles and owns every part of terminal emulation — alt-screen,
resize, scrollback, the lot. Charter fills the edges with its own processes and never
parses or draws the harness's pane. ADR 0018.
"""

from __future__ import annotations

from . import layout

__all__ = ["layout"]
```

`charter/frame/layout.py`:

```python
"""The frame's shape, decided before tmux is involved at all.

Pure on purpose. Everything that decides *what the frame looks like* lives here and
returns plain lists of strings, so the whole shape is under test on a machine with no
tmux, and so the argv rule below is enforced mechanically instead of by review.

**Nothing here ever joins argv.** Pinned against tmux 3.7c: `new-session … printf
'hello;touch INJ'` passed as separate arguments creates no file, and the same text as one
string creates it. Workspace, repo, branch and persona names all reach a frame from
committed files or `.git/HEAD`, so a joined string would be the `gh -F` bug again.
"""

from __future__ import annotations

from typing import NamedTuple

#: The order slots are dropped in as the terminal shrinks. Sides first — they cost the
#: harness columns, which is what a narrow terminal has none of — then the top, whose row
#: is worth less than the bottom's alerts.
_DROP_ORDER = ("left", "right", "top", "bottom")

#: Rows a horizontal panel occupies, and columns a vertical one does.
SLOT_SIZE = {"top": 1, "bottom": 1, "left": 22, "right": 22}


class Pane(NamedTuple):
    slot: str
    argv: list[str]
    size: int


def visible_slots(slots: list[str], cols: int, rows: int,
                  min_cols: int, min_rows: int) -> list[str]:
    """Which of *slots* fit in a *cols* x *rows* terminal.

    Degradation, not refusal: below the floor the harness simply gets the whole terminal,
    which is the same choice `statusline.render` makes when it runs out of width.
    """
    keep = list(slots)
    if cols < min_cols:
        keep = [s for s in keep if s not in ("left", "right")]
    if rows < min_rows:
        keep = [s for s in keep if s != "top"]
    if cols < min_cols // 2 or rows < min_rows // 2:
        keep = []
    return [s for s in slots if s in keep]


def _tmux(socket: str, *args: str) -> list[str]:
    return ["tmux", "-L", socket, *args]


def plan(*, slots: list[str], cols: int, rows: int, harness_argv: list[str],
         session: str, conf: str, socket: str, charter_argv: list[str]) -> list[list[str]]:
    """Every tmux command needed to build the frame, in order.

    Returns commands rather than running them, which is what makes the frame's shape
    assertable without a tmux binary anywhere near the test.
    """
    cmds: list[list[str]] = [
        _tmux(socket, "-f", conf, "new-session", "-d", "-s", session,
              "-x", str(cols), "-y", str(rows), "--", *harness_argv),
    ]
    for slot in slots:
        size = SLOT_SIZE[slot]
        direction = "-v" if slot in ("top", "bottom") else "-h"
        before = ["-b"] if slot in ("top", "left") else []
        cmds.append(_tmux(socket, "split-window", "-t", f"{session}:0.0",
                          direction, *before, "-l", str(size), "--",
                          *charter_argv, "panel", slot, "--session", session))
    return cmds
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_layout -v`
Expected: PASS, 8 tests

- [ ] **Step 5: Commit**

```bash
git add charter/frame/ tests/test_frame_layout.py
git commit -m "The frame's shape as a pure function, testable without tmux (#345)"
```

---

### Task 4: `frame/tmuxctl.py` — the only module that touches the binary

**Files:**
- Create: `charter/frame/tmuxctl.py`
- Test: `tests/test_frame_tmuxctl.py`

**Interfaces:**
- Consumes: `charter.frame.layout`
- Produces: `FLOOR: tuple[int, int]`, `version() -> tuple[int, int] | None`, `available() -> bool`, `meets_floor() -> bool`, `absent_message() -> str`, `below_floor_message(v) -> str`, `run(cmd: list[str]) -> int`

- [ ] **Step 1: Write the failing test**

```python
"""Everything that touches the tmux binary, in one module, so the rest is testable.

The messages are asserted because `Deficit` already settled what an absent capability has
to read like: naming the limit and the command that closes it, never a guess, because "a
remedy that does not exist costs more than an honest gap".
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter.frame import tmuxctl


class Version(unittest.TestCase):
    def test_a_release_version_parses(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux 3.7c"):
            self.assertEqual(tmuxctl.version(), (3, 7))

    def test_a_two_part_version_parses(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux 3.2"):
            self.assertEqual(tmuxctl.version(), (3, 2))

    def test_an_absent_binary_is_none_not_zero(self):
        """None is 'charter has nothing to say', which reads differently from 'version
        0.0' — the distinction `registry.deficits` makes for an unknown harness."""
        with mock.patch.object(tmuxctl, "_probe", return_value=None):
            self.assertIsNone(tmuxctl.version())

    def test_unparseable_output_is_none_rather_than_a_crash(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux next-3.9"):
            self.assertIsNone(tmuxctl.version())


class Floor(unittest.TestCase):
    def test_the_floor_is_the_version_display_popup_needs(self):
        self.assertEqual(tmuxctl.FLOOR, (3, 2))

    def test_a_new_enough_tmux_meets_the_floor(self):
        with mock.patch.object(tmuxctl, "version", return_value=(3, 7)):
            self.assertTrue(tmuxctl.meets_floor())

    def test_an_old_tmux_does_not(self):
        with mock.patch.object(tmuxctl, "version", return_value=(3, 0)):
            self.assertFalse(tmuxctl.meets_floor())


class Messages(unittest.TestCase):
    def test_the_absent_message_names_the_command_that_fixes_it(self):
        msg = tmuxctl.absent_message()
        self.assertIn("tmux", msg)
        self.assertIn("--no-frame", msg)

    def test_the_below_floor_message_names_both_versions(self):
        msg = tmuxctl.below_floor_message((3, 0))
        self.assertIn("3.0", msg)
        self.assertIn("3.2", msg)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_tmuxctl -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'charter.frame.tmuxctl'`

- [ ] **Step 3: Implement it**

`charter/frame/tmuxctl.py`:

```python
"""The only module in charter that runs tmux.

Kept alone so everything else — layout, panels, slots — is testable on a machine with no
tmux installed, and so there is exactly one place where the argv rule can be broken.
"""

from __future__ import annotations

import re
import shutil
import subprocess

#: `display-menu` arrived in 3.0 and `display-popup` in 3.2, and the frame's interaction
#: model uses both. Floor at the higher one and degrade below it rather than refuse.
FLOOR = (3, 2)

_VERSION = re.compile(r"^tmux (\d+)\.(\d+)")


def _probe() -> str | None:
    """`tmux -V`'s output, or ``None`` when there is no tmux to ask."""
    if not shutil.which("tmux"):
        return None
    try:
        out = subprocess.run(["tmux", "-V"], capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return None
    return out.stdout.strip() or None


def version() -> tuple[int, int] | None:
    """``(major, minor)``, or ``None`` when tmux is absent or unparseable.

    ``None`` is not "version zero": it says charter could not find out, which reads
    differently from "too old" and is answered with a different message.
    """
    raw = _probe()
    if not raw:
        return None
    m = _VERSION.match(raw)
    return (int(m.group(1)), int(m.group(2))) if m else None


def available() -> bool:
    return version() is not None


def meets_floor() -> bool:
    v = version()
    return bool(v and v >= FLOOR)


def absent_message() -> str:
    return ("charter's frame needs tmux, which is not on this machine.\n"
            "  install:  brew install tmux   (or your package manager)\n"
            "  without:  charter <harness> --no-frame  runs the harness bare")


def below_floor_message(v: tuple[int, int]) -> str:
    return (f"tmux {v[0]}.{v[1]} composes the frame, but its menu needs "
            f"tmux {FLOOR[0]}.{FLOOR[1]} — the frame starts with the hotkey disabled.")


def run(cmd: list[str]) -> int:
    """Run one tmux command. *cmd* is a LIST; this module never joins argv."""
    assert isinstance(cmd, list), "tmux argv must be a list — see frame/layout.py"
    return subprocess.run(cmd).returncode
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_tmuxctl -v`
Expected: PASS, 9 tests

- [ ] **Step 5: Commit**

```bash
git add charter/frame/tmuxctl.py tests/test_frame_tmuxctl.py
git commit -m "One module runs tmux, so the rest needs none to test (#345)"
```

---

### Task 5: `frame/state.py` — frame id, state directory, version file, reaping

**Files:**
- Create: `charter/frame/state.py`
- Test: `tests/test_frame_state.py`

**Interfaces:**
- Consumes: `charter.config.STATE_DIR`
- Produces: `frame_id(workspace: str, pid: int) -> str`, `frame_dir(fid: str) -> Path`, `bump(fid: str) -> None`, `version(fid: str) -> str`, `record_exit(fid: str, code: int) -> None`, `exit_code(fid: str) -> int | None`, `reap(live: set[str]) -> list[str]`

- [ ] **Step 1: Write the failing test**

```python
"""The frame's own state: who it is, when it last changed, how it ended.

Per frame rather than global, because two frames may run at once (one session each, named
by workspace and pid) and a shared version file would make each frame's panels redraw for
the other's activity.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from charter.frame import state

from _isolation import IsolatedPaths


class FrameId(unittest.TestCase):
    def test_the_id_carries_the_workspace_and_the_pid(self):
        fid = state.frame_id("harness-wrapper", 4242)
        self.assertIn("harness-wrapper", fid)
        self.assertIn("4242", fid)

    def test_a_hostile_workspace_name_cannot_escape_the_state_directory(self):
        """The id becomes a directory name. `contain.py` exists because a name read out
        of a file used to be joined onto a path with nothing in between."""
        fid = state.frame_id("../../etc", 1)
        self.assertNotIn("/", fid)
        self.assertNotIn("..", fid)


class Version(IsolatedPaths, unittest.TestCase):
    def test_a_fresh_frame_has_a_version(self):
        self.assertTrue(state.version("f-1"))

    def test_bumping_changes_it(self):
        before = state.version("f-1")
        state.bump("f-1")
        self.assertNotEqual(before, state.version("f-1"))

    def test_a_reader_never_sees_a_half_written_version(self):
        """Written with os.replace, so a panel reading mid-bump gets the old value whole
        rather than a torn one."""
        for _ in range(50):
            state.bump("f-1")
            self.assertTrue(state.version("f-1").strip())


class ExitCode(IsolatedPaths, unittest.TestCase):
    def test_an_unfinished_frame_has_no_exit_code(self):
        self.assertIsNone(state.exit_code("f-1"))

    def test_the_recorded_code_comes_back(self):
        state.record_exit("f-1", 42)
        self.assertEqual(state.exit_code("f-1"), 42)


class Reap(IsolatedPaths, unittest.TestCase):
    def test_a_directory_whose_session_is_gone_is_removed(self):
        state.bump("dead-1")
        state.bump("live-1")
        removed = state.reap({"live-1"})
        self.assertEqual(removed, ["dead-1"])
        self.assertFalse(state.frame_dir("dead-1").exists())
        self.assertTrue(state.frame_dir("live-1").exists())

    def test_a_live_frame_is_never_reaped_by_age(self):
        """A long-lived frame is exactly what an age heuristic would eat."""
        state.bump("old-1")
        self.assertEqual(state.reap({"old-1"}), [])


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_state -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'charter.frame.state'`

- [ ] **Step 3: Implement it**

`charter/frame/state.py`:

```python
"""What one frame knows about itself, on disk.

Under ``.charter/frame/<frame-id>/`` — per frame, never global, because two frames may run
at once and a shared version file would make each one's panels redraw for the other's work.
"""

from __future__ import annotations

import os
import re
import shutil
import time
from pathlib import Path

from .. import config

#: Anything outside this becomes an underscore. The id becomes a directory name, and
#: `contain.py` exists because a name read out of a file was once joined onto a path with
#: nothing in between.
_UNSAFE = re.compile(r"[^A-Za-z0-9._-]")


def frame_id(workspace: str, pid: int) -> str:
    """A stable id for one frame: the workspace it is for, and the launcher's pid.

    The same pair the tmux session is named for, so a directory on disk and a session in
    `tmux list-sessions` can always be matched up by eye.
    """
    safe = _UNSAFE.sub("_", workspace).strip("._-") or "frame"
    return f"{safe}-{int(pid)}"


def _root() -> Path:
    return Path(config.STATE_DIR) / "frame"


def frame_dir(fid: str) -> Path:
    d = _root() / _UNSAFE.sub("_", fid)
    d.mkdir(parents=True, exist_ok=True)
    return d


def bump(fid: str) -> None:
    """Record that the plane changed. Written whole, then moved into place.

    ``os.replace`` is atomic on the same filesystem, so a panel reading mid-bump gets the
    previous value entire rather than a truncated one.
    """
    d = frame_dir(fid)
    tmp = d / "version.tmp"
    tmp.write_text(f"{time.time_ns()}\n")
    os.replace(tmp, d / "version")


def version(fid: str) -> str:
    f = frame_dir(fid) / "version"
    try:
        return f.read_text().strip()
    except OSError:
        bump(fid)
        try:
            return (frame_dir(fid) / "version").read_text().strip()
        except OSError:
            return "0"


def record_exit(fid: str, code: int) -> None:
    d = frame_dir(fid)
    tmp = d / "exit.tmp"
    tmp.write_text(f"{int(code)}\n")
    os.replace(tmp, d / "exit")


def exit_code(fid: str) -> int | None:
    try:
        return int((frame_dir(fid) / "exit").read_text().strip())
    except (OSError, ValueError):
        return None


def reap(live: set[str]) -> list[str]:
    """Remove state for frames whose tmux session is gone. Returns what was removed.

    Never by age: a frame open for two days is a working frame, and an age heuristic
    would delete exactly that one.
    """
    root = _root()
    if not root.is_dir():
        return []
    removed = []
    for d in sorted(root.iterdir()):
        if d.is_dir() and d.name not in live:
            shutil.rmtree(d, ignore_errors=True)
            removed.append(d.name)
    return removed
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_state -v`
Expected: PASS, 9 tests

- [ ] **Step 5: Commit**

```bash
git add charter/frame/state.py tests/test_frame_state.py
git commit -m "A frame's own state: id, version, exit code, reaping (#345)"
```

---

### Task 6: The launcher — `charter <harness>`, bypass, and the exit code

**Files:**
- Create: `charter/commands_frame.py`
- Modify: `charter/cli.py` (import, `_add_frame_parsers(sub)`, call site near line 314)
- Test: `tests/test_frame_launcher.py`

**Interfaces:**
- Consumes: `harness.all()`, `frame.layout.plan`, `frame.tmuxctl`, `frame.state`, `config.FRAME`
- Produces: `cmd_launch(args) -> int`, `conf_text(*, hotkey, mouse, history_limit, status_path, harness_pane) -> str`, `bypass(argv: list[str]) -> int`

- [ ] **Step 1: Write the failing test**

```python
"""Starting a harness inside a frame, and the three ways that must not go wrong.

The exit code is asserted because it was measured wrong once: an attached
`tmux new-session` returns 0 whatever its command exited with (pinned against 3.7c), so
the launcher waits and reads a recorded status instead of exec'ing and hoping.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import commands_frame


class Bypass(unittest.TestCase):
    def test_a_pipe_gets_no_frame(self):
        """`charter claude -p "…" | jq` must be `claude -p "…" | jq`. A frame around a
        pipe is wrong, and exec keeps the exit code without any help."""
        with mock.patch("sys.stdout.isatty", return_value=False), \
             mock.patch("os.execvp") as ex:
            commands_frame.bypass(["claude", "-p", "hi"])
        ex.assert_called_once_with("claude", ["claude", "-p", "hi"])


class Conf(unittest.TestCase):
    def test_the_pane_died_hook_is_scoped_to_the_harness_pane(self):
        """pane-died fires for ANY pane. Unscoped, a dead panel would be reported as the
        agent's exit code."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=50000,
                                        status_path="/tmp/f/exit", harness_pane="%3")
        self.assertIn("%3", text)
        self.assertIn("pane_dead_status", text)

    def test_remain_on_exit_is_set_or_the_status_never_exists(self):
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=50000,
                                        status_path="/tmp/f/exit", harness_pane="%3")
        self.assertIn("remain-on-exit on", text)

    def test_mouse_is_off_unless_asked_for(self):
        off = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                       status_path="/x", harness_pane="%0")
        self.assertIn("set -g mouse off", off)
        on = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=1,
                                      status_path="/x", harness_pane="%0")
        self.assertIn("set -g mouse on", on)

    def test_history_limit_is_raised_above_the_tmux_default(self):
        """tmux ships 2000 lines, which becomes the harness's entire scrollback."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=50000,
                                        status_path="/x", harness_pane="%0")
        self.assertIn("history-limit 50000", text)


class MissingTmux(unittest.TestCase):
    def test_an_absent_tmux_names_the_remedy_and_does_not_start_a_frame(self):
        args = mock.Mock(harness="claude", rest=[], no_frame=False)
        with mock.patch("charter.frame.tmuxctl.available", return_value=False), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("builtins.print") as p:
            rc = commands_frame.cmd_launch(args)
        self.assertNotEqual(rc, 0)
        printed = " ".join(str(c) for c in p.call_args_list)
        self.assertIn("--no-frame", printed)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_launcher -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'charter.commands_frame'`

- [ ] **Step 3: Implement the launcher**

`charter/commands_frame.py`:

```python
"""`charter <harness>` — run the harness inside charter's frame.

The launcher does NOT exec tmux, and that is measured rather than stylistic: an attached
`tmux new-session` returns 0 whatever its command exited with (tmux 3.7c). So the status
is carried out of band by a pane-scoped `pane-died` hook, and this process waits for tmux,
reads the recorded code, and exits with it. `exec` survives only on the bypass path, where
there is no frame in the way.
"""

from __future__ import annotations

import os
import subprocess
import sys

from . import config, harness
from .frame import layout, state, tmuxctl


def bypass(argv: list[str]) -> int:
    """Run the harness with no frame at all — `exec`, so the exit code needs no help."""
    os.execvp(argv[0], argv)
    return 127  # unreachable; execvp either replaces this process or raises


def conf_text(*, hotkey: str, mouse: bool, history_limit: int,
              status_path: str, harness_pane: str) -> str:
    """The private tmux config for one frame. Never the operator's ~/.tmux.conf.

    The `pane-died` hook is scoped to the harness pane. Unscoped it fires for any pane,
    and a crashed panel would be reported to the operator as the agent's exit code.
    """
    return "\n".join([
        "set -g status off",
        f"set -g mouse {'on' if mouse else 'off'}",
        f"set -g history-limit {int(history_limit)}",
        "set -g escape-time 0",
        "set -g remain-on-exit on",
        "bind -n WheelUpPane if-shell -F -t = '#{mouse_any_flag}'"
        " 'send-keys -M' 'copy-mode -e; send-keys -M'",
        f"set-hook -p -t {harness_pane} pane-died "
        f"'run-shell \"echo #{{pane_dead_status}} > {status_path}\" ; kill-session'",
        "",
    ])


def _live_sessions(socket: str) -> set[str]:
    try:
        out = subprocess.run(["tmux", "-L", socket, "list-sessions", "-F", "#{session_name}"],
                             capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return set()
    return {line.strip() for line in out.stdout.splitlines() if line.strip()}


def cmd_launch(args) -> int:
    """One launcher, shared by every registered harness and by `charter frame --`."""
    h = next((x for x in harness.all() if x.cli_name == args.harness), None)
    argv = h.launch_argv(list(args.rest)) if h else list(args.rest)
    if not argv:
        print("charter frame: nothing to run — `charter frame -- <command>`", file=sys.stderr)
        return 2

    if getattr(args, "no_frame", False) or not sys.stdout.isatty():
        return bypass(argv)

    if not tmuxctl.available():
        print(tmuxctl.absent_message(), file=sys.stderr)
        return 1
    v = tmuxctl.version()
    if not tmuxctl.meets_floor():
        print(tmuxctl.below_floor_message(v), file=sys.stderr)

    frame = config.FRAME
    from . import workspace
    ws = workspace.resolve()
    fid = state.frame_id(ws, os.getpid())
    fdir = state.frame_dir(fid)
    state.reap(_live_sessions("charter"))
    state.bump(fid)

    cols, rows = os.get_terminal_size()
    slots = layout.visible_slots(frame["slots"], cols, rows,
                                 frame["min_cols"], frame["min_rows"])
    conf = fdir / "tmux.conf"
    conf.write_text(conf_text(hotkey=frame["hotkey"], mouse=frame["mouse"],
                              history_limit=frame["history_limit"],
                              status_path=str(fdir / "exit"), harness_pane="%0"))

    env = dict(os.environ, CHARTER_SESSION_ID=fid)
    if h:
        env["CHARTER_HARNESS"] = h.name
    for cmd in layout.plan(slots=slots, cols=cols, rows=rows, harness_argv=argv,
                           session=fid, conf=str(conf), socket="charter",
                           charter_argv=[sys.argv[0]]):
        subprocess.run(cmd, env=env)

    subprocess.run(["tmux", "-L", "charter", "attach", "-t", fid], env=env)
    code = state.exit_code(fid)
    state.reap(_live_sessions("charter"))
    return code if code is not None else 0
```

- [ ] **Step 4: Wire it into the CLI**

In `charter/cli.py`, add `commands_frame` to the `from . import (…)` block (line 9), then add this function beside `_add_harness_parser`:

```python
def _add_frame_parsers(sub) -> None:
    """One launcher per registered harness, plus the escape hatch.

    Generated from `harness.KINDS` rather than listed, which is the reason that registry
    exists: a harness added to it gets a launcher the day it is registered.
    """
    from . import commands_frame

    def _wire(parser, name):
        parser.add_argument("rest", nargs=argparse.REMAINDER,
                            help="Passed to the harness verbatim.")
        parser.add_argument("--no-frame", action="store_true",
                            help="Run the harness bare, with no charter frame.")
        parser.set_defaults(harness=name, func=commands_frame.cmd_launch)

    for h in harness.all():
        if not h.cli_name:
            continue
        p = sub.add_parser(h.cli_name,
                           help=f"Run {h.cli_name} inside charter's frame.")
        _wire(p, h.cli_name)

    fr = sub.add_parser("frame",
                        help="Run any command inside charter's frame — `charter frame -- <cmd>`.")
    _wire(fr, "")
```

Then call it in `build_parser()` beside the others (near line 314):

```python
    _add_frame_parsers(sub)
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_launcher tests.test_harness_launch tests.test_cli_smoke -v`
Expected: PASS

- [ ] **Step 6: Check the CLI actually grew the commands**

Run: `python -m charter frame --help` and `python -m charter claude --help`
Expected: both print help; `--no-frame` listed

- [ ] **Step 7: Commit**

```bash
git add charter/commands_frame.py charter/cli.py tests/test_frame_launcher.py
git commit -m "charter <harness> starts the frame, and carries the real exit code (#345)"
```

---

### Task 7: `frame/panel.py` — the panel runtime

**Files:**
- Create: `charter/frame/panel.py`
- Modify: `charter/cli.py` (add the internal `panel` parser)
- Test: `tests/test_frame_panel.py`

**Interfaces:**
- Consumes: `charter.frame.state.version`, `charter.frame.slots.render`
- Produces: `run(slot: str, fid: str, *, once: bool = False) -> int`, `should_redraw(seen: str, fid: str) -> bool`

- [ ] **Step 1: Write the failing test**

```python
"""A panel wakes because the agent did something, not because a timer went off.

The version file is a `stat` per tick, which is why the frame costs nothing at idle. A
FIFO was designed and rejected: opening one for write blocks until a reader exists, which
would put a hang inside the hook path.
"""

from __future__ import annotations

import unittest

from charter.frame import panel, state

from _isolation import IsolatedPaths


class Redraw(IsolatedPaths, unittest.TestCase):
    def test_an_unchanged_version_does_not_redraw(self):
        seen = state.version("f-1")
        self.assertFalse(panel.should_redraw(seen, "f-1"))

    def test_a_bump_asks_for_a_redraw(self):
        seen = state.version("f-1")
        state.bump("f-1")
        self.assertTrue(panel.should_redraw(seen, "f-1"))

    def test_a_missing_frame_redraws_rather_than_crashing(self):
        """A panel outliving its state directory must show something, not die and leave
        a hole in the frame."""
        self.assertTrue(panel.should_redraw("nothing-like-a-version", "never-existed"))


class Draw(IsolatedPaths, unittest.TestCase):
    def test_one_pass_writes_the_slot_and_returns(self):
        rc = panel.run("bottom", "f-1", once=True)
        self.assertEqual(rc, 0)

    def test_an_unknown_slot_is_refused_rather_than_drawn_blank(self):
        rc = panel.run("sideways", "f-1", once=True)
        self.assertNotEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_panel -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'charter.frame.panel'`

- [ ] **Step 3: Implement the runtime**

`charter/frame/panel.py`:

```python
"""One charter panel, owning one tmux pane.

Repaints whole. A five-row pane is a few hundred cells, so diffing would be optimising
something that is already free, and `tui.py` already truncates rather than wrapping when
the pane is narrow.
"""

from __future__ import annotations

import signal
import sys
import time

from . import slots, state

#: How often the version file is checked. A `stat` at this rate is indistinguishable from
#: zero, and it cannot hang the hook that writes it — which a FIFO could.
TICK = 0.2


def should_redraw(seen: str, fid: str) -> bool:
    try:
        return state.version(fid) != seen
    except Exception:
        return True


def _paint(slot: str, fid: str) -> None:
    sys.stdout.write("\x1b[H\x1b[2J" + slots.render(slot, fid))
    sys.stdout.flush()


def run(slot: str, fid: str, *, once: bool = False) -> int:
    if slot not in slots.SLOTS:
        print(f"charter panel: unknown slot '{slot}' "
              f"(known: {', '.join(sorted(slots.SLOTS))})", file=sys.stderr)
        return 2

    resized = {"flag": True}
    signal.signal(signal.SIGWINCH, lambda *_: resized.__setitem__("flag", True))

    seen = ""
    while True:
        if resized["flag"] or should_redraw(seen, fid):
            resized["flag"] = False
            seen = state.version(fid)
            _paint(slot, fid)
        if once:
            return 0
        time.sleep(TICK)
```

- [ ] **Step 4: Wire the internal command**

In `charter/cli.py`, inside `_add_frame_parsers`, append:

```python
    pn = sub.add_parser("panel")          # internal: one pane of a running frame
    pn.add_argument("slot")
    pn.add_argument("--session", dest="session", required=True)
    pn.set_defaults(func=lambda args: __import__(
        "charter.frame.panel", fromlist=["panel"]).run(args.slot, args.session))
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_panel -v`
Expected: PASS, 5 tests

- [ ] **Step 6: Commit**

```bash
git add charter/frame/panel.py charter/cli.py tests/test_frame_panel.py
git commit -m "A panel owns its pane and wakes on a version bump (#345)"
```

---

### Task 8: Liveness — the hooks bump the frame

**Files:**
- Modify: `charter/hooks.py` (one call in the `sessionstart`, `userpromptsubmit` and `posttooluse` handlers)
- Create: `charter/frame/notify.py`
- Test: `tests/test_frame_liveness.py`

**Interfaces:**
- Consumes: `charter.frame.state.bump`
- Produces: `notify.plane_changed() -> None` (debounced, never raises)

- [ ] **Step 1: Write the failing test**

```python
"""The frame updates because the agent did something.

`posttooluse` runs on every tool call, and `hooks.py` already treats that path as hot, so
the bump is debounced and swallows every error: a hook may cost a session its briefing,
never its turn.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter.frame import notify, state

from _isolation import IsolatedPaths


class Notify(IsolatedPaths, unittest.TestCase):
    def test_a_change_bumps_the_running_frame(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            before = state.version("f-1")
            notify._last["at"] = 0.0
            notify.plane_changed()
            self.assertNotEqual(before, state.version("f-1"))

    def test_a_second_bump_inside_the_debounce_window_is_skipped(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            notify._last["at"] = 0.0
            notify.plane_changed()
            first = state.version("f-1")
            notify.plane_changed()
            self.assertEqual(first, state.version("f-1"))

    def test_outside_a_frame_it_does_nothing_at_all(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            notify.plane_changed()   # must not raise

    def test_a_broken_state_directory_never_reaches_the_hook(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}), \
             mock.patch.object(state, "bump", side_effect=OSError("read-only")):
            notify._last["at"] = 0.0
            notify.plane_changed()   # must not raise


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_liveness -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'charter.frame.notify'`

- [ ] **Step 3: Implement it**

`charter/frame/notify.py`:

```python
"""Telling a running frame that the plane changed.

Called from hooks, so the two rules are absolute: it never raises, and it never costs the
hot path anything. `posttooluse` fires on every tool call.
"""

from __future__ import annotations

import os
import time

from . import state

#: At most one bump per this many seconds. A panel ticks at 0.2s, so a tighter debounce
#: buys nothing a reader could see.
DEBOUNCE = 0.25

_last = {"at": 0.0}


def plane_changed() -> None:
    """Bump the frame this process is running inside, if any. Never raises."""
    try:
        fid = os.environ.get("CHARTER_SESSION_ID")
        if not fid:
            return
        now = time.monotonic()
        if now - _last["at"] < DEBOUNCE:
            return
        _last["at"] = now
        state.bump(fid)
    except Exception:
        return
```

- [ ] **Step 4: Call it from the three handlers**

In `charter/hooks.py`, at the top of the `sessionstart`, `userpromptsubmit` and `posttooluse` handler functions (the ones registered in `_HANDLERS`), add:

```python
    from .frame import notify
    notify.plane_changed()
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_liveness -v && python -m unittest discover -s tests 2>&1 | tail -3`
Expected: PASS, then OK

- [ ] **Step 6: Commit**

```bash
git add charter/frame/notify.py charter/hooks.py tests/test_frame_liveness.py
git commit -m "The frame updates because the agent did something (#345)"
```

---

### Task 9: `frame/slots.py` — what top and bottom actually say

**Files:**
- Create: `charter/frame/slots.py`
- Test: `tests/test_frame_slots.py`

**Interfaces:**
- Consumes: `charter.statusline` helpers (`_todo_count`, `_alerts`, `_persona_line`), `charter.workspace.resolve`, `charter.tui`
- Produces: `SLOTS: dict[str, callable]`, `render(slot: str, fid: str) -> str`

- [ ] **Step 1: Write the failing test**

```python
"""Panels compose the renderers the status line already has.

Zones are not re-invented here: `statusline.py` already argues for identity in one place
and alerts in another, and a frame that split them differently would be a second layout to
keep in step with the first.
"""

from __future__ import annotations

import unittest

from charter import tui
from charter.frame import slots

from _isolation import IsolatedPaths


class Render(IsolatedPaths, unittest.TestCase):
    def test_top_names_the_workspace(self):
        out = slots.render("top", "f-1")
        self.assertTrue(out.strip())

    def test_bottom_renders(self):
        self.assertTrue(slots.render("bottom", "f-1").strip())

    def test_a_slot_never_exceeds_the_pane_width(self):
        """`tui.width` counts display cells, not characters — a wide glyph that fits by
        len() still wraps the pane and pushes the frame apart."""
        for slot in ("top", "bottom"):
            for line in slots.render(slot, "f-1").splitlines():
                with self.subTest(slot=slot):
                    self.assertLessEqual(tui.width(line), tui.term_width(default=80))

    def test_a_failing_renderer_yields_a_line_rather_than_an_exception(self):
        """A panel that raises leaves a hole in the frame; `statusline.render` makes the
        same promise for the same reason."""
        slots.SLOTS["boom"] = lambda fid: 1 / 0
        try:
            self.assertIn("charter", slots.render("boom", "f-1"))
        finally:
            del slots.SLOTS["boom"]


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_slots -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'charter.frame.slots'`

- [ ] **Step 3: Implement it**

`charter/frame/slots.py`:

```python
"""What each edge of the frame says.

Content comes from the renderers `statusline.py` already has — they are composed here,
never rewritten, so a fix to a repo row or an alert lands in both surfaces at once.
"""

from __future__ import annotations

from .. import tui


def _width() -> int:
    return max(20, tui.term_width(default=80, floor=20))


def _top(fid: str) -> str:
    """Identity: where you are, pinned or not, and who you are being."""
    from .. import __version__, statusline, workspace
    ws = workspace.resolve()
    src = workspace.source()
    pin = "*" if src == "$CHARTER_WORKSPACE" else ""
    persona = statusline._persona_line() or ""
    left = f" ⬢ {ws}{pin}"
    right = f"{persona}  charter {__version__} "
    return tui.truncate(f"{left}  {right}", _width())


def _bottom(fid: str) -> str:
    """What still wants attention, and how to act on it."""
    from .. import statusline, workspace
    ws = workspace.resolve()
    todos = statusline._todo_count(ws)
    alerts = statusline._alerts(ws)
    parts = [f"{todos} todo" + ("s" if todos != 1 else "")]
    parts.extend(alerts[:1])
    parts.append("F2 menu")
    return tui.truncate(" · ".join(p for p in parts if p), _width())


#: Every slot charter can draw. `panel.run` refuses a name that is not in here rather
#: than painting an empty pane, because an empty pane reads as a broken frame.
SLOTS = {"top": _top, "bottom": _bottom}


def render(slot: str, fid: str) -> str:
    """Draw *slot*, or a one-line explanation of why it could not be drawn.

    Never raises. A panel that dies leaves a hole in the frame, which is worse than a
    line saying what went wrong — the promise `statusline.render` already makes.
    """
    fn = SLOTS.get(slot)
    if fn is None:
        return tui.truncate(f" charter: unknown slot {slot}", _width())
    try:
        return fn(fid)
    except Exception as e:
        return tui.truncate(f" charter: {slot} unavailable ({type(e).__name__})", _width())
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_slots -v`
Expected: PASS, 4 tests

- [ ] **Step 5: Commit**

```bash
git add charter/frame/slots.py tests/test_frame_slots.py
git commit -m "Top and bottom compose the renderers the status line already has (#345)"
```

---

### Task 10: The menu, and the boundary names never cross

**Files:**
- Create: `charter/frame/menu.py`
- Modify: `charter/commands_frame.py` (add `cmd_action`), `charter/cli.py` (`frame action`)
- Test: `tests/test_frame_menu.py`

**Interfaces:**
- Consumes: `charter.frame.state.frame_dir`
- Produces: `build(fid: str) -> list[tuple[str, str]]` (label, opaque id), `menu_argv(fid, socket) -> list[str]`, `resolve(fid: str, action_id: str) -> list[str] | None`

- [ ] **Step 1: Write the failing test**

```python
"""A workspace name must never reach a tmux command string.

`display-menu` takes commands tmux parses and runs, and workspace, repo, branch and
persona names all come from committed files or `.git/HEAD`. Charter shipped a fix for
this exact shape one release ago — a branch name reaching `gh -F` and making it read a
file — and its conclusion was that the fix is the mechanism, not the value. So menu items
carry opaque ids and charter resolves them in-process.
"""

from __future__ import annotations

import unittest

from charter.frame import menu

from _isolation import IsolatedPaths


HOSTILE = 'x" ; run-shell "touch /tmp/pwned'


class OpaqueIds(IsolatedPaths, unittest.TestCase):
    def test_a_menu_item_carries_an_id_not_a_name(self):
        menu.record(fid="f-1", entries=[(HOSTILE, ["charter", "ws", "use", HOSTILE])])
        argv = menu.menu_argv("f-1", "charter")
        flat = " ".join(argv)
        self.assertNotIn("run-shell", flat)
        self.assertNotIn("/tmp/pwned", flat)

    def test_the_id_resolves_back_to_the_real_command_in_process(self):
        menu.record(fid="f-1", entries=[(HOSTILE, ["charter", "ws", "use", HOSTILE])])
        entries = menu.build("f-1")
        self.assertEqual(len(entries), 1)
        _label, action_id = entries[0]
        self.assertEqual(menu.resolve("f-1", action_id),
                         ["charter", "ws", "use", HOSTILE])

    def test_an_unknown_id_resolves_to_nothing(self):
        self.assertIsNone(menu.resolve("f-1", "not-an-id"))

    def test_an_id_is_only_ever_id_shaped(self):
        menu.record(fid="f-1", entries=[(HOSTILE, ["charter", "true"])])
        for _label, action_id in menu.build("f-1"):
            self.assertRegex(action_id, r"^a[0-9]+$")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_menu -v`
Expected: FAIL — `ModuleNotFoundError: No module named 'charter.frame.menu'`

- [ ] **Step 3: Implement it**

`charter/frame/menu.py`:

```python
"""The frame's menu, built so no name ever reaches tmux's parser.

`display-menu` and `display-popup -E` take commands **tmux** parses and executes. Every
interesting label in charter — a workspace, a repo, a branch, a persona — is read out of a
committed file or `.git/HEAD`, which is exactly the input class that made `gh -F` open a
file on someone's machine. Quoting it for tmux would be an arms race against a parser with
`;` separation, `#{}` expansion and two quote styles.

So the command tmux runs is always `charter frame action a<N>`, and charter looks the real
argv up in its own state. Labels are still shown — a label is drawn, never executed — but
they are sanitised of the one thing that could confuse a menu: newlines.
"""

from __future__ import annotations

import json

from . import state


def _table(fid: str):
    return state.frame_dir(fid) / "actions.json"


def record(*, fid: str, entries: list[tuple[str, list[str]]]) -> None:
    """Store this frame's menu as ``{id: argv}``, and the labels beside it."""
    data = {f"a{i}": {"label": label.replace("\n", " ")[:60], "argv": argv}
            for i, (label, argv) in enumerate(entries)}
    _table(fid).write_text(json.dumps(data))


def build(fid: str) -> list[tuple[str, str]]:
    """``(label, opaque id)`` for each entry, in order."""
    try:
        data = json.loads(_table(fid).read_text())
    except (OSError, ValueError):
        return []
    return [(v["label"], k) for k, v in sorted(data.items())]


def resolve(fid: str, action_id: str) -> list[str] | None:
    """The real argv for an id, or ``None``. The only place an id becomes a command."""
    try:
        data = json.loads(_table(fid).read_text())
    except (OSError, ValueError):
        return None
    entry = data.get(action_id)
    if not isinstance(entry, dict):
        return None
    argv = entry.get("argv")
    return argv if isinstance(argv, list) and all(isinstance(a, str) for a in argv) else None


def menu_argv(fid: str, socket: str) -> list[str]:
    """The `display-menu` invocation for this frame. Ids only — never a name."""
    cmd = ["tmux", "-L", socket, "display-menu", "-T", "charter"]
    for i, (label, action_id) in enumerate(build(fid)):
        cmd += [label, str(i + 1), f"run-shell 'charter frame action {action_id}'"]
    return cmd
```

- [ ] **Step 4: Add the action command**

Append to `charter/commands_frame.py`:

```python
def cmd_action(args) -> int:
    """Run a menu entry by its opaque id. The only path from a menu to a command."""
    from .frame import menu

    fid = os.environ.get("CHARTER_SESSION_ID", "")
    argv = menu.resolve(fid, args.action_id)
    if not argv:
        print(f"charter frame action: unknown action '{args.action_id}'", file=sys.stderr)
        return 2
    return subprocess.run(argv).returncode
```

In `charter/cli.py`, inside `_add_frame_parsers`, after the `frame` parser:

```python
    frsub = fr.add_subparsers(dest="frame_command")
    act = frsub.add_parser("action")   # internal: a menu entry, by opaque id
    act.add_argument("action_id")
    act.set_defaults(func=commands_frame.cmd_action)
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `python -m unittest tests.test_frame_menu -v`
Expected: PASS, 4 tests

- [ ] **Step 6: Commit**

```bash
git add charter/frame/menu.py charter/commands_frame.py charter/cli.py tests/test_frame_menu.py
git commit -m "Menu entries carry opaque ids, so no name reaches tmux's parser (#345)"
```

---

### Task 11: Docs, news, ADR, and the doctor row

**Files:**
- Create: `docs/frame.md`, `docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md`, `docs/news/unreleased-charter-runs-the-harness.md`
- Modify: `pyproject.toml` (force-include `docs/frame.md`), `charter/doctor.py`
- Test: `tests/test_frame_doctor.py` (the docs test already exists and will fail without the force-include)

**Interfaces:**
- Consumes: `charter.frame.tmuxctl`
- Produces: a `doctor` row reporting tmux presence and version

- [ ] **Step 1: Write the failing test**

```python
"""tmux is a frame prerequisite, not a harness ceiling.

Filing it under `harness.deficits` would claim claude-code cannot do something it does
perfectly well — `tests/test_doctor_absent_is_not_health.py` already draws that line.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import doctor


class FrameRow(unittest.TestCase):
    def test_a_present_tmux_reports_its_version(self):
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)):
            row = doctor.frame_row()
        self.assertIn("3.7", row)

    def test_an_absent_tmux_is_named_not_silent(self):
        with mock.patch("charter.frame.tmuxctl.version", return_value=None):
            row = doctor.frame_row()
        self.assertIn("tmux", row)

    def test_tmux_is_not_reported_as_a_harness_deficit(self):
        from charter import harness
        for h in harness.all():
            with self.subTest(harness=h.name):
                self.assertNotIn("tmux", " ".join(d.key for d in h.deficits))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run it to verify it fails**

Run: `python -m unittest tests.test_frame_doctor -v`
Expected: FAIL — `AttributeError: module 'charter.doctor' has no attribute 'frame_row'`

- [ ] **Step 3: Add the doctor row**

Append to `charter/doctor.py`:

```python
def frame_row() -> str:
    """One line on whether `charter <harness>` can compose a frame here.

    Its own check rather than a `Deficit`: tmux is a prerequisite of the frame, not a
    ceiling of any harness, and filing it as a deficit would tell the reader that their
    harness is limited when it is not.
    """
    from .frame import tmuxctl

    v = tmuxctl.version()
    if v is None:
        return "frame    tmux not found — `charter <harness>` needs it (brew install tmux)"
    ok = "" if v >= tmuxctl.FLOOR else f" — menu needs {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]}"
    return f"frame    tmux {v[0]}.{v[1]}{ok}"
```

Call it from the same place the other rows are printed in `doctor`'s report body.

- [ ] **Step 4: Write `docs/frame.md`**

```markdown
# The frame

`charter claude` runs the harness inside a frame charter composes: the harness in the
middle, charter's panels on the edges. It works on every harness, which is the point — a
status line is Claude Code's surface, and Codex and opencode have none.

    charter claude              # or codex, opencode
    charter frame -- <cmd>      # anything charter has never met
    charter claude --no-frame   # bare, no frame at all

## What it needs

tmux 3.2 or newer. tmux composes the rectangles and does every part of terminal emulation;
charter fills the edges and never draws or parses the harness's pane (ADR 0018).

## What changes inside the frame

Scrollback is tmux's copy-mode rather than your terminal's. The frame raises
`history-limit` to 50 000 and binds the wheel to copy-mode, but it is not the same thing
as your terminal's own buffer, and it is the difference people notice first.

Mouse is off by default: `set -g mouse on` takes over drag-select, so turning it on trades
your terminal's copy behaviour for clickable panels. `[frame] mouse = true` when you want
that trade.

Charter never touches `~/.tmux.conf`. Outside tmux it starts a private server; inside tmux
it builds the same layout as a new window in your own server, so there is no nesting and
no second prefix key.

## Configuring it

```toml
[frame]
slots = ["top", "bottom"]
mouse = false
hotkey = "F2"
history-limit = 50000
min-cols = 100
min-rows = 20
```

Below `min-cols`/`min-rows` the side panels drop, then the top, and below the floor the
harness simply gets the whole terminal.
```

- [ ] **Step 5: Force-include the docs page**

In `pyproject.toml`, in `[tool.hatch.build.targets.wheel.force-include]`, add in alphabetical order:

```toml
"docs/frame.md" = "charter/_docs/frame.md"
```

- [ ] **Step 6: Write ADR 0018 and the news entry**

`docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md` records the measurement (1.85 MB/s against 25.2 MB/s end to end, 2.4 MB/s pyte parse against ~37 MB/s tmux), that both arms worked so feasibility was never the question, and the maintenance argument (a 120-line widget with no cursor, on a parser last released 2023-11-12, in a project shipping `dependencies = []`).

`docs/news/unreleased-charter-runs-the-harness.md` uses the existing entry format, with a `check:` probe that is read-only, uses charter's own argv, and cannot hang:

```markdown
---
version: unreleased
headline: charter can run the harness, and draw around it
check: frame --probe
---
```

- [ ] **Step 7: Run the whole suite**

Run: `python -m unittest discover -s tests -v 2>&1 | tail -5`
Expected: OK — including `tests/test_docs_show.py`, which fails on a `docs/*.md` that is not force-included

- [ ] **Step 8: Commit**

```bash
git add docs/ pyproject.toml charter/doctor.py tests/test_frame_doctor.py
git commit -m "Document the frame, record ADR 0018, and report tmux in doctor (#345)"
```

---

## Self-review notes

**Spec coverage.** Command surface → Task 6. Non-TTY bypass → Task 6. Missing tmux → Tasks 4, 6. `cli_name`/`launch_argv` → Task 1. Collision test → Task 1. Process model and private conf → Task 6. Inside-tmux new window → **not yet covered; see below.** Identity and `CHARTER_SESSION_ID` → Task 6. `CHARTER_WORKSPACE` never exported → Task 6 (asserted in Task 1's constraint list, worth its own test when the inside-tmux path lands). Liveness → Task 8. Slots → Task 9. Focus pinned, menu, opaque ids → Task 10. Mouse default → Tasks 2, 6. Scrollback → Task 6. Degradation → Task 3. Exit code → Task 6. Panel crash policy → **not yet covered; see below.** Config → Task 2. Docs/news/ADR/doctor → Task 11.

**Two gaps this plan leaves open, deliberately, as the v1.1 slice:**

1. **The inside-tmux path** (`$TMUX` set → new window in the operator's server, prefix-scoped keys). Every piece it needs exists after Task 6; it is one branch in `cmd_launch` plus its own test file, and it is the first thing to add.
2. **Panel respawn with backoff** (`remain-on-exit` already keeps a dead panel visible with its error, which is the half that matters; three-attempt respawn is the other half).

Both are named here rather than silently dropped, because a plan that omits them reads as if the spec were fully covered.
