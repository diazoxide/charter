"""What charter SAYS about a nested plane, now that resolution handles it.

`charter.toml` is tracked, so every clone of a plane is a plane, and `charter clone` puts
clones at ``workspaces/<ws>/<repo>``. `find_root` hops outward through ``workspaces/`` so
the plane holding the vault wins.

That leaves three states, and each has to read differently — which is the whole content of
this file:

* **flat** — no nesting. Say nothing. Most planes, every render.
* **hopped** — standing in a nested plane, acting on the one above it. A *notice*: nothing
  is going astray, but a correction charter makes silently is one nobody can argue with.
* **overridden** — ``$CHARTER_ROOT`` pinned inside the nested plane, so the hop was refused
  and charter really is writing there. A *warning*: this is the hazard #140 described.

0.36.0 marked the nesting in the status line header, because charter was resolving to the
inner plane and the operator had to be told. Once resolution is right, the status line
shows the right plane and a row about a corrected situation is furniture on every render.
The old alert said "memory and vault go to X, not Y" — after the hop that sentence is
false.

Every plane is identified by **path**, never ``Path.name``: clone charter into its own
plane and both directories are called ``charter``, which is how that alert came out as
"memory and vault go to charter, not charter" and told nobody anything (#200).
"""

from __future__ import annotations

import io
import re
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, commands_worktree, config, root, statusline, util
from tests._isolation import PersonaIso


class NestedCase(PersonaIso):
    """A plane whose root really does sit under another plane's ``workspaces/``.

    Built on disk rather than mocked: the behaviour is a path relationship, and a stubbed
    answer would pass while the arithmetic was wrong.
    """

    def dirs(self) -> tuple[Path, Path]:
        # A name that cannot collide with anything the renderer already prints — the brand
        # row carries the word "charter", so an outer plane called that would let a test
        # pass on the wrong line.
        outer = self.tmp / "umbrella"
        inner = outer / "workspaces" / "w1" / "inner"
        inner.mkdir(parents=True, exist_ok=True)
        (outer / "charter.toml").write_text("schema = 1\n")
        (inner / "charter.toml").write_text("schema = 1\n")
        return outer, inner

    def hopped(self) -> tuple[Path, Path]:
        """Production shape after the hop: ROOT is the OUTER, and ``NESTED_ORIGIN`` records
        the plane the operator is standing in."""
        outer, inner = self.dirs()
        config.use(outer)
        config.NESTED_ORIGIN = inner.resolve()
        config.PERSONAS_DIR.mkdir(parents=True, exist_ok=True)
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        return outer, inner

    def overridden(self) -> tuple[Path, Path]:
        """``$CHARTER_ROOT`` pinned inside the nested plane: ROOT *is* the inner one."""
        outer, inner = self.dirs()
        config.use(inner)
        config.NESTED_ORIGIN = config.ROOT
        config.PERSONAS_DIR.mkdir(parents=True, exist_ok=True)
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        return outer, inner

    def flat(self) -> None:
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.use(self.tmp)
        config.NESTED_ORIGIN = None


class TestTheFixtureIsReal(NestedCase):
    def test_the_nesting_is_actually_detected(self):
        """A precondition, not a feature. If `enclosing_plane` cannot see this shape, every
        other test in the file passes for the wrong reason."""
        outer, inner = self.dirs()
        self.assertEqual(root.enclosing_plane(inner), outer.resolve())

    def test_resolution_hops_to_the_outer_plane(self):
        outer, inner = self.dirs()
        self.assertEqual(root.find_root(inner), outer.resolve())


class TestTheStatusLineStaysQuietAfterTheHop(NestedCase):
    def test_no_nested_marker_rides_the_header(self):
        self.hopped()
        header = [ln for ln in _plain(statusline.render(_payload())).splitlines()
                  if "⬢" in ln]
        self.assertTrue(header, "no header row rendered")
        self.assertNotIn("nested", " ".join(header).lower())

    def test_the_false_alert_row_is_not_rendered(self):
        """"memory and vault go to X, not Y" is false once they go to the outer plane."""
        self.hopped()
        self.assertNotIn("memory and vault", _plain(statusline.render(_payload())))

    def test_the_duplicate_wrapper_is_gone(self):
        """`_nested_under`'s docstring warns this rule "was implemented twice, here and
        there, which is how two surfaces come to disagree about what nested means". 0.36.0
        made it three."""
        self.assertFalse(hasattr(statusline, "_nested_plane_mark"))


class TestTheOverrideStillWarns(NestedCase):
    def test_the_alert_row_fires(self):
        """Refusing the hop is exactly the state #140 warned about, and the only one where
        a warning is still true."""
        self.overridden()
        self.assertIn("memory and vault", _plain(statusline.render(_payload())))

    def test_both_planes_are_identified_by_path_not_name(self):
        """The defect that made the original alert useless. Here the two planes have
        DIFFERENT names, so a bare-name rendering would pass — the assertion is on the
        separator that only a path can contain."""
        self.overridden()
        row = [ln for ln in _plain(statusline.render(_payload())).splitlines()
               if "memory and vault" in ln][0]
        self.assertIn("/", row)


class TestStatusNamesTheHop(NestedCase):
    def run_status(self) -> str:
        buf = io.StringIO()
        with redirect_stdout(buf), redirect_stderr(io.StringIO()):
            commands.cmd_status(SimpleNamespace(workspace=None, all=False))
        return buf.getvalue()

    def test_the_hop_is_stated(self):
        """A correction charter makes silently is one nobody can argue with — and "where am
        I" is this command's whole job."""
        outer, inner = self.hopped()
        out = self.run_status()
        self.assertIn(str(inner.resolve()), out.replace("~", str(Path.home())))
        self.assertIn("acting on", out)

    def test_a_flat_plane_says_nothing(self):
        self.flat()
        said = self.run_status().lower()
        self.assertNotIn("nested", said)
        self.assertNotIn("acting on", said)


class TestTheMisleadingErrorIsCorrected(NestedCase):
    def resolve_err(self) -> str:
        buf = io.StringIO()
        with redirect_stderr(buf), redirect_stdout(io.StringIO()):
            commands_worktree._resolve(SimpleNamespace(repo="charter", workspace=None))
        return buf.getvalue()

    def test_it_states_the_hop_when_one_happened(self):
        """The message the operator met was "'charter' isn't cloned in workspace 'default'"
        — for a clone they were standing in, in a plane they never chose. Naming which
        plane answered is what makes the rest of the message readable."""
        outer, inner = self.hopped()
        self.assertIn("acting on", self.resolve_err())

    def test_the_plane_is_named_before_the_clone_advice(self):
        """Order is the correction: the clone tip is the fallback, and when charter can see
        the plane moved that is the likelier explanation."""
        self.hopped()
        err = self.resolve_err()
        self.assertLess(err.index("acting on"), err.index("Clone it first"))

    def test_the_clone_advice_survives(self):
        """Not-cloned stays possible either way — charter names the likelier cause without
        deleting the other one (ADR 0009)."""
        self.hopped()
        self.assertIn("Clone it first", self.resolve_err())

    def test_a_flat_plane_keeps_the_original_message(self):
        self.flat()
        err = self.resolve_err()
        self.assertIn("isn't cloned in workspace", err)
        self.assertNotIn("acting on", err)


def _payload() -> dict:
    return {"session_id": "s", "workspace": {"current_dir": str(config.ROOT)}}


def _plain(text: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", text)


if __name__ == "__main__":
    unittest.main()
