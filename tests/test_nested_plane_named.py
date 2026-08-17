"""A nested plane is named where the wrong impression forms, not only in `doctor`.

`charter.toml` is tracked, so every clone of a plane is itself a plane — and `charter
clone` puts clones at `workspaces/<ws>/<repo>`. Standing in one, resolution stops at the
first marker walking up and the inner plane shadows the outer (#140).

`root.enclosing_plane` already detects this and `doctor` already reports it. #140 chose
that deliberately: resolution stays put, because sometimes the inner plane IS the one you
mean — charter's own dogfooding clones charter into a workspace. **This does not revisit
that choice.** What it fixes is the silence around it.

Observed (#200): the status line rendered `⬢ default · ws 3` — a different plane, a
different workspace count, the active-persona marker gone — with nothing saying the plane
had changed. The operator saw `default`, could not account for it, and had no reason to
suspect the plane rather than the workspace. `doctor` would have explained it, but nobody
runs `doctor` because the status line looks odd; the status line is the surface read every
turn. This is the complaint `workspace.source` was already extended for — "why are you in
default workspace again?" — one level up, and ADR 0013's second rule aimed at that line.

Three surfaces, and no more. A nesting notice on every command is how a real signal becomes
furniture:

* the **status line**, where the wrong impression forms;
* **`charter status`**, whose whole job is "where am I";
* the **`isn't cloned in workspace X` error**, which is worse than silent — it names a
  cause that is not real and advises cloning into a plane nobody asked for (ADR 0009).
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, commands_worktree, config, root, statusline, workspace
from tests._isolation import PersonaIso


class NestedCase(PersonaIso):
    """A plane whose root really does sit under another plane's ``workspaces/``.

    Built on disk rather than mocked, because `enclosing_plane`'s whole job is a path
    relationship — a stubbed answer would pass while the real arithmetic was wrong.
    """

    def nest(self) -> Path:
        # A name that cannot collide with anything the renderer already prints — the brand
        # row carries the word "charter", so an outer plane called that would let a test
        # pass on the wrong line.
        outer = self.tmp / "umbrella"
        inner = outer / "workspaces" / "w1" / "inner"
        inner.mkdir(parents=True, exist_ok=True)
        (outer / "charter.toml").write_text("schema = 1\n")
        (inner / "charter.toml").write_text("schema = 1\n")
        config.use(inner)
        config.PERSONAS_DIR.mkdir(parents=True, exist_ok=True)
        (config.WORKSPACES_DIR).mkdir(parents=True, exist_ok=True)
        return outer

    def flat(self) -> None:
        """The ordinary case: a plane with no plane above it."""
        (self.tmp / "charter.toml").write_text("schema = 1\n")


class TestTheFixtureIsReal(NestedCase):
    def test_the_nested_plane_is_actually_detected(self):
        """A precondition, not a feature. If `enclosing_plane` cannot see this fixture,
        every other test in the file passes for the wrong reason."""
        outer = self.nest()
        # `.resolve()` on both sides: `enclosing_plane` canonicalises, and on macOS the
        # temp dir arrives as /var/… while the resolved answer is /private/var/…. Comparing
        # unresolved would fail on a symlink rather than on the behaviour under test.
        self.assertEqual(root.enclosing_plane(config.ROOT), outer.resolve())

    def test_a_flat_plane_is_not_detected_as_nested(self):
        self.flat()
        self.assertIsNone(root.enclosing_plane(config.ROOT))


class TestTheStatusLineNamesIt(NestedCase):
    def test_a_flat_plane_gets_no_mark(self):
        """Most planes are flat, and a notice on them would be furniture."""
        self.flat()
        self.assertIsNone(statusline._nested_plane_mark())

    def test_a_nested_plane_is_marked(self):
        self.nest()
        self.assertIsNotNone(statusline._nested_plane_mark())

    def test_the_mark_names_the_outer_plane(self):
        """"Nested" alone does not tell you which plane you meant to be in — the operator
        needs the name to recognise it as theirs."""
        outer = self.nest()
        self.assertIn(outer.name, statusline._nested_plane_mark())

    def test_it_reaches_the_rendered_header(self):
        """A helper nothing renders is the same silence this fixes."""
        outer = self.nest()
        out = statusline.render({"session_id": "s", "workspace": {"current_dir": str(config.ROOT)}})
        self.assertIn(outer.name, _plain(out))

    def test_it_never_takes_the_render_down(self):
        """`render` runs every turn and degrades to a bare brand rather than showing the
        operator a traceback. An unreadable ancestor must not change that."""
        real = root.enclosing_plane
        root.enclosing_plane = lambda *a, **k: (_ for _ in ()).throw(OSError("nope"))
        self.addCleanup(setattr, root, "enclosing_plane", real)
        self.assertIsNone(statusline._nested_plane_mark())

    def test_it_rides_the_header_row(self):
        """Q2's whole reason for choosing the header over a warning row: the release
        immediately before this one was spent bounding the status line's height, and the
        correction belongs against the token being misread rather than three rows below
        it. Asserted as "the outer name shares a line with the `⬢` header" rather than by
        counting rows, because a flat plane and a nested one are different fixtures and
        their row counts are not comparable."""
        outer = self.nest()
        named = [ln for ln in _plain(statusline.render(_payload())).splitlines()
                 if outer.name in ln]
        self.assertTrue(named, "the outer plane is not named anywhere")
        self.assertTrue(any("⬢" in ln for ln in named),
                        f"the mark did not ride the header row: {named}")


class TestStatusNamesIt(NestedCase):
    def run_status(self) -> str:
        buf = io.StringIO()
        with redirect_stdout(buf), redirect_stderr(io.StringIO()):
            commands.cmd_status(SimpleNamespace(workspace=None, all=False))
        return buf.getvalue()

    def test_a_nested_plane_is_named(self):
        """"Where am I" is the whole job of this command, and the plane is the outermost
        part of that answer."""
        outer = self.nest()
        self.assertIn(outer.name, self.run_status())

    def test_a_flat_plane_says_nothing_about_nesting(self):
        self.flat()
        self.assertNotIn("nested", self.run_status().lower())


class TestTheMisleadingErrorIsCorrected(NestedCase):
    def resolve_err(self) -> str:
        buf = io.StringIO()
        with redirect_stderr(buf), redirect_stdout(io.StringIO()):
            commands_worktree._resolve(SimpleNamespace(repo="charter", workspace=None))
        return buf.getvalue()

    def test_it_names_the_plane_when_nested(self):
        """The message the operator actually met was "'charter' isn't cloned in workspace
        'default'" — for a clone that exists, in a plane they never chose. Advising a
        second clone is charter guessing at a cause it can see is wrong (ADR 0009)."""
        outer = self.nest()
        self.assertIn(outer.name, self.resolve_err())

    def test_the_plane_is_named_before_the_clone_advice(self):
        """Order is the correction. The clone tip is the fallback for the flat case; when
        charter can see the plane is nested, that is the likelier cause and has to be read
        first."""
        self.nest()
        err = self.resolve_err()
        self.assertLess(err.lower().index("nested"), err.index("Clone it first"))

    def test_the_clone_advice_survives(self):
        """Not-cloned is still possible in a nested plane — charter names the more likely
        cause without deleting the other one."""
        self.nest()
        self.assertIn("Clone it first", self.resolve_err())

    def test_a_flat_plane_keeps_the_original_message(self):
        self.flat()
        err = self.resolve_err()
        self.assertIn("isn't cloned in workspace", err)
        self.assertNotIn("nested", err.lower())


def _payload() -> dict:
    return {"session_id": "s", "workspace": {"current_dir": str(config.ROOT)}}


def _plain(text: str) -> str:
    import re
    return re.sub(r"\x1b\[[0-9;]*m", "", text)


if __name__ == "__main__":
    unittest.main()
