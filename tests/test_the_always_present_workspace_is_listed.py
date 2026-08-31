"""`charter workspace list` lists the workspace the plane is standing in — #745.

**The report came from the frame and the defect is in the CLI.** A UX audit found that the
`F2` workspace picker offers `default` on a plane whose `workspaces/` holds four other
names, that choosing it succeeds, and that `charter workspace list` then does not list
where the frame is. It read as "the picker offers something that is not a workspace".

**It is a workspace.** Measured on a plane built by `charter init`, before anything else:

* `workspace.resolve()` answers `default` and `workspace.source()` says
  `default (nothing selected)` — it is the rung the resolution ladder terminates on, so
  every plane starts there and there is no other answer for it to give.
* `charter workspace use default` accepts it on a plane that has never made one, says so
  in a comment older than this issue, and `workspace.ensure` creates the directory then.
* `charter clone <repo> -w default` creates it too, which is what the frame's own repo
  panel tells the operator to run.
* `frame/switch.workspaces` folds it in for exactly that reason, and the docs' own picker
  example draws `default` as an ordinary row beside `alpha`, `beta` and `zebra`.

So `default` is a real workspace that is a real DIRECTORY on some planes and not others —
`charter init` does not make it, unlike `personas/steward`. Everything in charter agreed
about that except the listing, which reads directories.

**What that cost, in one screen.** The table draws a two-cell "you are here" inset, and on
a plane with workspaces where nobody has selected one the mark landed on no row at all::

    Active workspace: default  (via default (nothing selected))

      WORKSPACE  MODE   CLONES  REPOS
      alpha      local  0       —
      beta       local  0       —

A header naming a workspace, a column reserved for marking it, and no row for it.

**Nothing is created to fix it, here or in `charter init`.** `[workspace] default` names
this workspace, so `init` would be baking a directory for a value the operator may change
in the charter.toml `init` itself just wrote — and every route that puts something IN a
workspace calls `workspace.ensure` already. A row costs nothing and cannot go stale.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_workspace, config, workspace
from charter.frame import switch

from tests._isolation import PersonaIso


class TheListingAndThePickerAnswerTheSameQuestion(PersonaIso, unittest.TestCase):
    """One plane, two surfaces, one set of names."""

    def setUp(self) -> None:
        super().setUp()
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)

    def make(self, *names: str) -> None:
        for n in names:
            workspace.workspace_dir(n).mkdir(parents=True, exist_ok=True)

    def listing(self) -> list[str]:
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            commands_workspace.cmd_workspace_list(SimpleNamespace())
        return out.getvalue().splitlines()

    def names_in(self, lines: list[str]) -> list[str]:
        """The WORKSPACE cell of every body row, in order.

        Read off the drawn table rather than off `list_workspaces`, because what is being
        asserted is what the operator sees — a fix that changed the model and not the
        renderer would pass a test that asked the model.
        """
        body = [ln for ln in lines if ln and not ln.lstrip().startswith("WORKSPACE")
                and not ln.startswith("Active workspace:")]
        return [ln[2:].split()[0] for ln in body if ln[2:].strip()]

    def test_the_active_workspace_has_a_row_and_it_is_the_marked_one(self):
        """The reported screen. `default` is where the plane resolves and where the frame
        stands after the picker, and it was the one name the table could not show."""
        self.make("alpha", "beta")
        lines = self.listing()
        self.assertEqual(self.names_in(lines), ["alpha", "beta", "default"])
        marked = [ln for ln in lines if ln.startswith("* ")]
        self.assertEqual(len(marked), 1, lines)
        self.assertIn("default", marked[0])

    def test_the_listing_names_exactly_what_the_picker_offers(self):
        """The property the two surfaces disagreeing about is what produced the report.
        `frame/switch.workspaces` is what the `F2` picker and the launch prompt both draw,
        so a name in one and not the other is a route to a workspace one of them denies
        exists."""
        self.make("alpha", "beta", "gamma")
        self.assertEqual(self.names_in(self.listing()), switch.workspaces())

    def test_the_mark_always_has_a_row_to_land_on(self):
        """The general statement of the defect: the table reserves two cells for "you are
        here", so a listing that can resolve to a name it does not draw has a column that
        is sometimes about nothing. Asked across the three shapes a plane comes in."""
        for made in ((), ("alpha",), ("alpha", "default")):
            with self.subTest(made=made):
                self.make(*made)
                lines = self.listing()
                self.assertIn(workspace.resolve(), self.names_in(lines))

    def test_a_plane_that_declares_another_default_gets_no_extra_row(self):
        """`[workspace] default` names this workspace, and folding in a literal `default`
        rather than the configured one would put a row on the table for a name nothing on
        the plane resolves to — the defect this fixes, inverted."""
        self.make("alpha", "beta")
        with mock.patch.object(config, "DEFAULT_WORKSPACE", "alpha"):
            names = self.names_in(self.listing())
        self.assertEqual(names, ["alpha", "beta"])

    def test_listing_creates_nothing(self):
        """A read stays a read. The row is the answer, not a directory made to justify one
        — `charter init` deliberately does not make it either, and a listing that did
        would be creating a workspace as a side effect of being asked what exists.
        """
        self.make("alpha")
        self.listing()
        self.assertFalse(workspace.workspace_dir(config.DEFAULT_WORKSPACE).exists())
        self.assertEqual(workspace.list_workspaces(), ["alpha"])

    def test_a_plane_with_no_workspaces_still_says_how_to_make_one(self):
        """"There is one row and it is the fallback" and "somebody has made a workspace"
        are different states, and a one-row table cannot tell them apart. The nudge that
        was the whole of the old empty-plane answer stays, beside the row."""
        err = io.StringIO()
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            commands_workspace.cmd_workspace_list(SimpleNamespace())
        self.assertEqual(self.names_in(out.getvalue().splitlines()),
                         [config.DEFAULT_WORKSPACE])
        self.assertIn("charter workspace create", err.getvalue())

    def test_a_plane_that_has_made_it_gets_one_row_and_not_two(self):
        """The fold-in is conditional, and the condition is the one `switch.workspaces`
        already uses. A second `default` row would be the same name twice with the mark on
        one of them."""
        self.make("alpha", config.DEFAULT_WORKSPACE)
        self.assertEqual(self.names_in(self.listing()),
                         ["alpha", config.DEFAULT_WORKSPACE])
