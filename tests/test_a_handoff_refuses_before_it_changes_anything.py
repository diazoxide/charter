"""`charter handoff` refuses everything it can before it changes anything.

A handoff creates a workspace, records a todo, opens a chat and sends it a first message.
Charter fails toward NO CHANGE, so every reason to refuse is asked before the first write:
a name that cannot be a workspace, a flag pair that contradicts itself, a persona nobody
has, a brief that is missing, unreadable or credential-shaped, a shell that is not a chat
in a frame, and the background seam's own refusals (`commands_frame.background_refusal`,
which Task 1 exports for exactly this).

Each refusal says the rule worked and names the fix in the same breath (CONTEXT.md), and
`_nothing_changed` is asserted beside the sentence — a refusal that already wrote the todo
is not a refusal.

`tests/test_charter_handoff_opens_a_chat_that_starts_working.py` is the other half: what a
handoff that is NOT refused does, in order.
"""

from __future__ import annotations

import io
import os
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import (cli, commands_frame, commands_handoff, config, dispatch, todos,
                     workspace)
from charter.frame import state

from tests import _tmuxsocket
from tests._isolation import PlaneIso
from tests.test_the_chat_bars_plus_makes_a_chat import _a_chat

#: The brief every case sends unless it says otherwise. Two lines, because the first line
#: becomes the todo's title and the rest must be shown to stay out of it.
BRIEF = "Fix the widget\nIt breaks on resize.\n"

#: A live specimen of one `hooks._SECRET_CHECKS` rule, taken from
#: `tests/test_a_secret_guard_names_the_kind_not_the_value.py` rather than invented here —
#: the handoff refusal reuses that classifier (plan Open question 20) and a second shape
#: list is how the two come to disagree.
AWS_KEY = "AKIAQ7VZ3RJHT2LMNPQR"


class _ATerminal:
    """A stdin that says it is a terminal and screams if anything reads it.

    `charter handoff` asks `isatty()` BEFORE any read, because a read on a terminal blocks
    the turn forever with nothing on screen to say why.
    """

    def isatty(self) -> bool:
        return True

    def read(self, *a):
        raise AssertionError("read a terminal")

    @property
    def buffer(self):
        raise AssertionError("read a terminal")


class _AHandoffFromAlpha(PlaneIso):
    """Chat `alpha.1` in workspace `alpha`, a `beta` workspace beside it, the background
    seam stood in for.

    Fixture only — no test methods here, so importing it into the other Task 2 module
    costs that module nothing. `self.bg` and `self.open` are the two Task 1 seams: what
    this suite is about is which refusals `cmd_handoff` reaches and what it wrote by then,
    not whether tmux really opens a window (Task 1's real-tmux module answers that).
    """

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {
            "CHARTER_SESSION_ID": "alpha.1", "TMUX_PANE": "%1",
            "CHARTER_HARNESS": "claude-code"}, clear=True))
        workspace.ensure("alpha")
        workspace.ensure("beta")
        _a_chat("alpha.1", ws="alpha", pane="%1")
        #: The real one, kept so a case about a REAL refusal can hand it back as the
        #: stand-in's `side_effect` — `enterContext` owns the patcher, and stopping it by
        #: hand would leave the class's own cleanup stopping it twice.
        self.real_background_refusal = commands_frame.background_refusal
        self.bg = self.enterContext(
            mock.patch("charter.commands_frame.background_refusal", return_value=""))
        #: What the stand-in seam answers, and what runs while it is "opening".
        self.opened = commands_frame.Opened(True, "beta.1", "")
        self.during_open: list = []
        self.open = self.enterContext(
            mock.patch("charter.commands_frame.open_in_background", side_effect=self._open))

    def _open(self, ws, *, caller, first_message, persona=""):
        for fn in self.during_open:
            fn()
        if self.opened.ok:
            # A launcher that opened a chat left its directory and its harness pane behind;
            # `state.record_brief` needs one to write into.
            _a_chat(self.opened.chat, ws=ws, pane="%9")
        return self.opened

    def _handoff(self, ws="beta", brief=BRIEF, *, stdin=None, **flags):
        """Run `cmd_handoff` with *brief* on stdin; return ``(rc, stdout, stderr)``."""
        args = SimpleNamespace(workspace=ws, create=flags.get("create", False),
                               vision=flags.get("vision"), persona=flags.get("persona"))
        if stdin is None:
            raw = brief if isinstance(brief, bytes) else brief.encode()
            stdin = io.TextIOWrapper(io.BytesIO(raw), encoding="utf-8")
        out, err = io.StringIO(), io.StringIO()
        was, sys.stdin = sys.stdin, stdin
        try:
            with redirect_stdout(out), redirect_stderr(err):
                rc = commands_handoff.cmd_handoff(args)
        finally:
            sys.stdin = was
        return rc, out.getvalue(), err.getvalue()

    def _nothing_changed(self) -> None:
        self.assertFalse((config.WORKSPACES_DIR / "gamma").exists())
        self.assertEqual(todos.count_open("beta"), 0)
        self.assertEqual(sorted(state._root().rglob("brief")), [])
        self.assertEqual([o for o in dispatch._read_all() if o.get("event") == "handoff"], [])
        self.open.assert_not_called()


class AHandoffRefusesBeforeItChangesAnything(_AHandoffFromAlpha):
    # -- the workspace and its flags ---------------------------------------------------

    def test_a_name_that_cannot_be_a_workspace_is_refused(self):
        rc, _out, err = self._handoff("../x")
        self.assertEqual(rc, 1)
        self.assertIn("cannot name a workspace", err)
        self._nothing_changed()

    def test_a_vision_without_create_is_refused(self):
        rc, _out, err = self._handoff("beta", vision="v")
        self.assertEqual(rc, 1)
        self.assertIn("--vision describes a workspace this call creates", err)
        self._nothing_changed()

    def test_create_without_a_vision_is_refused(self):
        rc, _out, err = self._handoff("gamma", create=True)
        self.assertEqual(rc, 1)
        self.assertIn("--create needs --vision", err)
        self._nothing_changed()

    def test_create_on_an_existing_workspace_is_refused(self):
        rc, _out, err = self._handoff("beta", create=True, vision="v")
        self.assertEqual(rc, 1)
        self.assertIn("already exists", err)
        self._nothing_changed()

    def test_create_on_the_always_present_workspace_is_refused_even_with_no_directory(self):
        """`default` is selectable whether or not its directory exists — `cmd_workspace_use`'s
        rule, asked here rather than invented again."""
        self.assertNotIn(config.DEFAULT_WORKSPACE, workspace.list_workspaces())
        rc, _out, err = self._handoff(config.DEFAULT_WORKSPACE, create=True, vision="v")
        self.assertEqual(rc, 1)
        self.assertIn("already exists", err)
        self._nothing_changed()

    def test_an_unknown_workspace_is_refused_naming_create_and_vision(self):
        rc, _out, err = self._handoff("gamma")
        self.assertEqual(rc, 1)
        self.assertIn("--create --vision", err)
        self._nothing_changed()

    def test_an_unknown_persona_is_refused_listing_the_ones_there_are(self):
        self.make_persona("forge")
        rc, _out, err = self._handoff("beta", persona="forj")
        self.assertEqual(rc, 1)
        self.assertIn("no persona 'forj' — have: forge", err)
        self._nothing_changed()

    # -- the brief ---------------------------------------------------------------------

    def test_a_terminal_on_stdin_is_refused_without_reading_it(self):
        rc, _out, err = self._handoff("beta", stdin=_ATerminal())
        self.assertEqual(rc, 1)
        self.assertIn("stdin here is a terminal", err)
        self.assertIn("<<'BRIEF'", err)
        self._nothing_changed()

    def test_a_brief_that_is_not_utf8_is_refused(self):
        rc, _out, err = self._handoff("beta", brief=b"\xff\xfe")
        self.assertEqual(rc, 1)
        self.assertIn("not UTF-8", err)
        self._nothing_changed()

    def test_an_empty_brief_is_refused(self):
        rc, _out, err = self._handoff("beta", brief=" \n\n")
        self.assertEqual(rc, 1)
        self.assertIn("the brief on stdin is empty", err)
        self._nothing_changed()

    def test_a_brief_carrying_a_credential_is_refused_by_kind_not_value(self):
        rc, _out, err = self._handoff("beta", brief=f"Fix the widget\nuse {AWS_KEY} for S3\n")
        self.assertEqual(rc, 1)
        self.assertIn("AWS access key", err)
        self.assertNotIn(AWS_KEY, err)
        self.assertNotIn("AKIA", err)
        self._nothing_changed()

    # -- the frame -----------------------------------------------------------------------

    def test_outside_a_frame_it_refuses_and_prints_the_command_to_run(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}, clear=True):
            rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 1)
        self.assertIn("not a chat in a charter frame", err)
        self.assertIn("charter claude --workspace beta", err)
        self.assertIn("⟨handoff from chat none", err)
        self._nothing_changed()

    def test_a_chat_whose_pane_is_not_this_process_is_outside_a_frame(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "alpha.1", "TMUX_PANE": "%7",
                                          "CHARTER_HARNESS": "claude-code"}, clear=True):
            rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 1)
        self.assertIn("not a chat in a charter frame", err)
        self._nothing_changed()

    def test_inside_your_own_tmux_it_refuses_and_prints_the_command(self):
        state.record_server("alpha.1", _tmuxsocket.OPERATOR_SOCKET)
        rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 1)
        self.assertIn("a tmux you already had", err)
        self.assertIn("charter claude --workspace beta", err)
        self._nothing_changed()

    def test_the_printed_command_creates_the_workspace_first_when_asked_to(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}, clear=True):
            rc, _out, err = self._handoff("gamma", create=True, vision="a thing")
        self.assertEqual(rc, 1)
        self.assertIn("charter workspace create gamma --vision 'a thing' && "
                      "charter claude --workspace gamma", err)
        self._nothing_changed()

    def test_the_printed_command_carries_the_persona_asked_for(self):
        self.make_persona("forge")
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}, clear=True):
            rc, _out, err = self._handoff("beta", persona="forge")
        self.assertEqual(rc, 1)
        self.assertIn("CHARTER_PERSONA=forge charter claude", err)
        self._nothing_changed()

    def test_the_printed_command_spells_opencodes_prompt_flag(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode"}, clear=True):
            rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 1)
        self.assertIn("charter opencode --workspace beta --prompt", err)
        self._nothing_changed()

    def test_a_harness_nothing_names_leaves_the_word_to_fill_in(self):
        """No `$CHARTER_HARNESS` and no `[harness] default`: charter prints the command with
        `<harness>` where the word goes, and says that is what it did rather than picking."""
        with mock.patch.dict(os.environ, {}, clear=True), \
                mock.patch.object(config, "HARNESS", {}):
            rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 1)
        self.assertIn("charter <harness> --workspace beta", err)
        self.assertIn("which harness", err)
        self._nothing_changed()

    # -- the background seam --------------------------------------------------------------

    def test_a_refusal_from_the_background_seam_changes_nothing(self):
        self.bg.return_value = "cannot open a chat in 'beta': X"
        rc, _out, err = self._handoff("beta")
        self.assertEqual(rc, 1)
        self.assertIn("cannot open a chat in 'beta': X", err)
        self._nothing_changed()

    def test_a_brief_that_fits_alone_but_not_stamped_says_what_was_counted(self):
        """The seam measures the STAMPED message — the stamp line, a blank line, the brief —
        so a brief under the bound can still be over once it is stamped. The seam never saw
        a brief and cannot say that; `charter handoff` says it."""
        self.bg.side_effect = self.real_background_refusal   # the real bound
        cap = commands_frame.FIRST_MESSAGE_MAX_BYTES
        brief = "Fix the widget\n" + "x" * (cap - len("Fix the widget\n"))
        self.assertEqual(len(os.fsencode(brief)), cap)
        rc, _out, err = self._handoff("beta", brief=brief)
        self.assertEqual(rc, 1)
        self.assertIn(f"past {cap} bytes", err)
        self.assertIn("the stamp line, a blank line", err)
        self._nothing_changed()


class TheCommandIsReachable(unittest.TestCase):
    """`charter handoff` is a word the CLI parses, with exactly the flags the spec names."""

    def test_the_word_parses_to_the_command(self):
        args = cli.build_parser().parse_args(
            ["handoff", "beta", "--create", "--vision", "v", "--persona", "forge"])
        self.assertIs(args.func, commands_handoff.cmd_handoff)
        self.assertEqual(args.workspace, "beta")
        self.assertIs(args.create, True)
        self.assertEqual(args.vision, "v")
        self.assertEqual(args.persona, "forge")

    def test_the_workspace_is_always_named(self):
        """The permission prompt has to say where the chat goes, and `.` says nothing."""
        with self.assertRaises(SystemExit), redirect_stderr(io.StringIO()):
            cli.build_parser().parse_args(["handoff"])

    def test_there_is_no_brief_file_flag(self):
        """A prompt that shows a path is an approval of a path, not of the brief."""
        with self.assertRaises(SystemExit), redirect_stderr(io.StringIO()):
            cli.build_parser().parse_args(["handoff", "beta", "--brief-file", "b.md"])

    def test_there_is_no_harness_flag(self):
        with self.assertRaises(SystemExit), redirect_stderr(io.StringIO()):
            cli.build_parser().parse_args(["handoff", "beta", "--harness", "codex"])

    def test_there_is_no_repo_flag(self):
        with self.assertRaises(SystemExit), redirect_stderr(io.StringIO()):
            cli.build_parser().parse_args(["handoff", "beta", "--repo", "charter"])


class EveryRefusalSaysSomethingDifferent(_AHandoffFromAlpha):
    def test_every_refusal_says_something_different(self):
        """A refusal a reader cannot tell from another is a refusal that teaches nothing."""
        self.make_persona("forge")
        said = [
            self._handoff("../x")[2],
            self._handoff("beta", vision="v")[2],
            self._handoff("gamma", create=True)[2],
            self._handoff("beta", create=True, vision="v")[2],
            self._handoff("gamma")[2],
            self._handoff("beta", persona="forj")[2],
            self._handoff("beta", stdin=_ATerminal())[2],
            self._handoff("beta", brief=b"\xff\xfe")[2],
            self._handoff("beta", brief=" \n\n")[2],
            self._handoff("beta", brief=f"Fix it\n{AWS_KEY}\n")[2],
        ]
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}, clear=True):
            said.append(self._handoff("beta")[2])
        state.record_server("alpha.1", _tmuxsocket.OPERATOR_SOCKET)
        said.append(self._handoff("beta")[2])
        state.record_server("alpha.1", commands_frame.SOCKET)   # back in charter's own tmux
        self.bg.return_value = "cannot open a chat in 'beta': X"
        said.append(self._handoff("beta")[2])
        self.assertNotIn("", said)
        self.assertEqual(len(set(said)), len(said), said)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
