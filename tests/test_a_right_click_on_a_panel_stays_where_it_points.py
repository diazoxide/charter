"""`[frame] mouse = true` stops taking the keyboard off the harness on a RIGHT click (#848).

#634 is this file's sibling and the difference between them is the whole reason this one
exists. tmux's default `MouseDown1Pane` is two commands — `select-pane -t = \\; send-keys
-M` — so charter could write them out verbatim in the else-branch of its own panel test and
leave every pane it did not create exactly as tmux left it. Read back off a real 3.7c,
tmux's default for the RIGHT button is not two commands::

    MouseDown3Pane  if-shell -F -t = "#{||:#{mouse_any_flag},…}"
                      { select-pane -t = ; send-keys -M }
                      { display-menu -T "…" -t = -x M -y M … }

The branch that FORWARDS the report selects the pane first, and `#{mouse_any_flag}` is 1
for every charter panel that declared `click` or `scroll` — the panel asks its own terminal
to report, which is what makes it clickable at all — so with `[frame] mouse = true` every
right click on a panel takes the keyboard before the byte arrives. #846 made right-clicking
a thing operators do, which is what turned a pre-existing tmux behaviour into a defect.

**The else-branch cannot be written out, and that is measured rather than asserted.** It is
a page-long `display-menu` built out of `#{mouse_word}`, `#{mouse_line}`,
`#{mouse_hyperlink}`, `#{pane_marked}`, `#{pane_floating_flag}` and `#{pane_mode}`, and it
**differs between the tmux versions charter supports**: 1849 characters of `{}` command
blocks on 3.7c against 1378 of backslash-escaped `"…"` on a 3.2 built from the release
tarball, the 3.7c one carrying hyperlink and floating-pane rows the floor has never heard
of. A hard-coded copy would be wrong on one of them the day it was written.

So charter **wraps rather than replaces**: it reads the server's own binding back with
`list-keys -T root` and re-emits it as the else-branch of its own panel test. Three
functions hold that and this file asks about all three — the parse
(`commands_frame._menu_button_default`), the argv it feeds
(`_menu_button_argv`), and the read that joins them (`_menu_button_bind_argv`).
`tests/test_frame_input_reaches_a_component.py` is the real-tmux half: it can say what tmux
does with a real right click and cannot say whether charter ever read the binding.

**The measurement that chose the wrap over #634's own shape** — real server, real client on
a real pty, `mouse on`, three panes (a marked panel, the harness, and one more split by
hand standing in for the operator's), SGR button-2 reports injected as a reporting terminal
sends them. **tmux 3.7c and tmux 3.2 answered identically in all nine cells**::

    bind                       right-click a panel   right-click harness  right-click own split
    tmux's own default         delivered, MOVED      untouched            tmux's pane menu
    `MouseDown1Pane`'s shape   delivered, unchanged  untouched            MOVED, and NO MENU
    charter's (the wrap)       delivered, unchanged  untouched            tmux's pane menu

The middle row is #634's fix applied verbatim to this button, and it is the trade #848
refused: `select-pane -t =; send-keys -M` is tmux's WHOLE default for button 1 and only the
forwarding HALF of its default for button 3, so writing it out here deletes tmux's own pane
menu — Copy Line, Paste, Horizontal Split, Kill, Zoom — from the harness and from every
pane the operator split themselves, inside charter's own window, to fix a focus steal that
one click puts right.

**The issue's third open question, settled by measurement rather than by reasoning:
`list-keys -T root MouseDown3Pane` cannot be the read.** Asking `list-keys` for one key by
name prints the binding on a real 3.2 and prints **nothing at all, at rc 0**, on a real
3.7c. So the whole root table is listed and the line is picked out of it, which is what
:data:`commands_frame._MENU_BIND_RE` is for and why it has to carry two spellings.

**And `M-MouseDown3Pane` is not an answer either.** It is tmux's unconditional
`display-menu` and it is present on both versions, so it would be somewhere for the pane
menu to go — but it is a documented affordance the operator would have to be told about, in
exchange for a right-click that stopped doing what every other tmux does. The wrap costs
them nothing and needs no doc line at all.
"""

from __future__ import annotations

import subprocess
import unittest
from unittest import mock

from charter import commands_frame
from charter.frame import tmuxctl

from tests._tmuxsocket import OPERATOR_SOCKET

#: One real `list-keys -T root` line off a real tmux 3.7c, its `display-menu` cut after the
#: first two rows — the length is the one thing about it this file does not need, and the
#: real-tmux half reads the whole thing off a real server rather than off a literal here.
#: What IS verbatim is the shape: two spaces after `bind-key`, the column padding before
#: the command, and `{}` command blocks.
_LINE_37C = (
    'bind-key  -T root MouseDown3Pane            if-shell -F -t = '
    '"#{||:#{mouse_any_flag},#{&&:#{pane_in_mode},#{?#{m/r:(copy|view)-mode,'
    '#{pane_mode}},0,1}}}" { select-pane -t = ; send-keys -M } '
    '{ display-menu -T "#[align=centre]#{pane_index} (#{pane_id})" -t = -x M -y M '
    "'' Kill X { kill-pane } Respawn R { respawn-pane -k } }")

#: The same line off a 3.2 built from the release tarball — `tmuxctl.FLOOR`. One space
#: after `bind-key`, different padding, and the branches are backslash-escaped `"…"`
#: strings rather than `{}` blocks. **This is why the else-branch is sourced**: no single
#: literal charter could hold is right on both machines.
_LINE_32 = (
    'bind-key -T root MouseDown3Pane       if-shell -F -t = '
    '"#{||:#{mouse_any_flag},#{&&:#{pane_in_mode},#{?#{m/r:(copy|view)-mode,'
    '#{pane_mode}},0,1}}}" "select-pane -t= ; send -M" '
    '"display-menu -t= -xM -yM -T \\"#[align=centre]#{pane_index} (#{pane_id})\\" '
    '\'\' Kill X kill-pane Respawn R \\"respawn-pane -k\\""')


def _cmd(line: str) -> str:
    """Everything the sample line says after the key — what the parse must hand back."""
    return line.split("MouseDown3Pane", 1)[1].strip()


class TheLineCharterPicksOutOfTmuxsOwnTable(unittest.TestCase):
    """`_menu_button_default` against the two listings a supported tmux really prints."""

    def test_the_command_comes_back_off_a_real_3_7c_listing(self):
        self.assertEqual(commands_frame._menu_button_default(_LINE_37C), _cmd(_LINE_37C))

    def test_the_command_comes_back_off_a_real_3_2_listing(self):
        """The floor, whose padding and quoting are both different. A parse written
        against one version's spacing alone passes the case above and fails here."""
        self.assertEqual(commands_frame._menu_button_default(_LINE_32), _cmd(_LINE_32))

    def test_it_is_found_among_the_whole_table_and_not_only_alone(self):
        """The read lists the WHOLE root table (`list-keys -T root MouseDown3Pane` prints
        nothing on 3.7c), so the line arrives with two dozen others around it."""
        listing = "\n".join([
            "bind-key  -T root MouseDown1Pane            select-pane -t = \\; send-keys -M",
            _LINE_37C,
            "bind-key  -T root WheelUpPane               if-shell -F \"#{x}\" { a } { b }",
        ])
        self.assertEqual(commands_frame._menu_button_default(listing), _cmd(_LINE_37C))

    def test_an_operators_own_repeat_flag_does_not_hide_the_line(self):
        """`bind -n -r MouseDown3Pane …` reads back with the flag between `bind-key` and
        `-T`, measured on 3.7c. The flag is the operator's and the command after it is
        still the thing to wrap."""
        line = "bind-key -r -T root MouseDown3Pane          display-message hi"
        self.assertEqual(commands_frame._menu_button_default(line), "display-message hi")

    def test_a_binding_in_another_key_table_is_not_this_key(self):
        """`bind -T copy-mode MouseDown3Pane …` is a real thing an operator can write, and
        wrapping it here would install a copy-mode binding into the ROOT table — where it
        would run against panes that are not in copy mode at all."""
        line = "bind-key  -T copy-mode MouseDown3Pane       send-keys -X copy-pipe"
        self.assertIsNone(commands_frame._menu_button_default(line))

    def test_another_key_is_not_this_one(self):
        """`M-MouseDown3Pane` ends with this key's name and is tmux's UNCONDITIONAL
        `display-menu` — the one line in the table most likely to be mistaken for this one,
        and wrapping it would leave the steal in place while charter reported success."""
        line = ("bind-key  -T root M-MouseDown3Pane          display-menu -T \"x\" -t = "
                "-x M -y M Kill X { kill-pane }")
        self.assertIsNone(commands_frame._menu_button_default(line))

    def test_a_key_the_operator_unbound_is_left_unbound(self):
        """`unbind -n MouseDown3Pane` says this key does nothing, and a key that does
        nothing runs no `select-pane`: there is no steal to fix. Inventing a binding here
        would be charter putting back what the operator removed."""
        self.assertIsNone(commands_frame._menu_button_default(
            "bind-key  -T root MouseDown1Pane            select-pane -t =\n"))

    def test_nothing_at_all_is_not_a_crash(self):
        """A server that answered with an empty table — or with output charter could not
        read (`tmuxctl.DECODE_ERRORS`) — degrades to the behaviour every charter before
        this one had, rather than to a traceback out of a launcher that has already
        started the harness."""
        self.assertIsNone(commands_frame._menu_button_default(""))

    def test_charters_own_wrap_is_never_wrapped_again(self):
        """The second launch on a shared socket, which is the ordinary case rather than an
        edge one — a root key table is server-wide, so frame two reads what frame one
        installed.

        Each pass would nest another copy of a page-long `display-menu` inside the last,
        so the ninth frame on a socket would carry a kilobyte of tmux config per launch
        and the operator's own binding would be nine layers down. The test is for
        `_PANEL_OPTION` in the command text, which is in no default binding of tmux's on
        either version.
        """
        wrapped = ("bind-key  -T root MouseDown3Pane            if-shell -F -t = "
                   '"#{@charter_panel}" { send-keys -M } { ' + _cmd(_LINE_37C) + " }")
        self.assertIsNone(commands_frame._menu_button_default(wrapped))

    def test_the_marker_it_looks_for_is_the_one_charter_writes(self):
        """Spelled out of the constant rather than as a literal, deliberately and against
        this suite's usual rule: what the re-wrap guard has to agree with is the string
        `_menu_button_argv` PUTS in the binding, so a rename that moved both together must
        not be red — and a rename that moved only one must be."""
        self.assertIn(commands_frame._PANEL_OPTION,
                      " ".join(commands_frame._menu_button_argv(socket="charter",
                                                                default="x")))


class TheBindCharterBuilds(unittest.TestCase):
    """`_menu_button_argv` — the whole argv, element by element."""

    def test_the_argv_is_the_one_that_was_measured(self):
        """Every element as a literal, which is this file's sibling's rule (#547): an
        expectation assembled from the same constants the code assembles it from agrees
        with any of them."""
        self.assertEqual(
            commands_frame._menu_button_argv(socket="charter", default="display-menu -T x"),
            ["tmux", "-L", "charter", "bind-key", "-n", "MouseDown3Pane",
             "if-shell", "-F", "-t", "=", "#{@charter_panel}",
             "send-keys -M", "display-menu -T x"])

    def test_a_panel_is_forwarded_to_and_never_selected(self):
        """The true branch is `send-keys -M` ALONE. A `select-pane` that crept back into it
        would restore the exact defect, and every other assertion here would still pass —
        the bind would still be conditional, still carry the mark, still wrap the
        default."""
        argv = commands_frame._menu_button_argv(socket="charter", default="anything")
        self.assertEqual(argv[-2], "send-keys -M")

    def test_the_else_branch_is_what_the_server_already_had_byte_for_byte(self):
        """The whole of the fix. A branch charter edited, trimmed or re-quoted is a branch
        charter is now responsible for being right about on every tmux there will ever
        be."""
        default = _cmd(_LINE_37C)
        argv = commands_frame._menu_button_argv(socket="charter", default=default)
        self.assertEqual(argv[-1], default)

    def test_nothing_is_joined(self):
        """`tmuxctl.server_argv`'s rule, and here it is what makes the quoting question go
        away entirely: a joined string is shell-interpreted by tmux and a separate argv is
        not, so a page of `{}` blocks and `''` separators reaches `if-shell` verbatim with
        nothing between it and tmux's own parser."""
        argv = commands_frame._menu_button_argv(
            socket="charter", default="a ; b 'c' \"d\" { e } $(f) `g`")
        self.assertEqual(argv[-1], "a ; b 'c' \"d\" { e } $(f) `g`")
        self.assertTrue(all(isinstance(part, str) for part in argv))

    def test_it_reaches_the_server_it_was_asked_about(self):
        """Charter talks to two servers and binds keys on exactly one of them
        (`tmuxctl.server_argv`). A socket PATH is the operator's own tmux, and this is the
        head that decides which one a `bind-key` lands on."""
        argv = commands_frame._menu_button_argv(socket=OPERATOR_SOCKET, default="x")
        self.assertEqual(argv[:3], ["tmux", "-S", OPERATOR_SOCKET])


class TheReadThatJoinsThem(unittest.TestCase):
    """`_menu_button_bind_argv` — the one call that needs a running tmux."""

    @staticmethod
    def _answering(stdout: str, code: int = 0):
        seen: list[list[str]] = []

        def fake(action, argv, **kw):
            seen.append(argv)
            return subprocess.CompletedProcess(argv, code, stdout=stdout, stderr="")

        return seen, mock.patch("charter.frame.tmuxctl.run", side_effect=fake)

    def test_it_asks_for_the_whole_root_table(self):
        """And not for the one key. `list-keys -T root MouseDown3Pane` prints nothing at
        all on a real 3.7c — rc 0, no output — so a read narrowed to the key would answer
        "nothing bound" on the version charter is developed against and leave the fix
        silently off."""
        seen, patched = self._answering(_LINE_37C)
        with patched:
            commands_frame._menu_button_bind_argv(socket="charter")
        self.assertEqual(seen, [["tmux", "-L", "charter", "list-keys", "-T", "root"]])

    def test_what_the_server_said_becomes_the_else_branch(self):
        seen, patched = self._answering(_LINE_37C)
        with patched:
            argv = commands_frame._menu_button_bind_argv(socket="charter")
        self.assertEqual(argv[-1], _cmd(_LINE_37C))
        self.assertEqual(argv[3:6], ["bind-key", "-n", "MouseDown3Pane"])

    def test_a_server_that_would_not_answer_binds_nothing(self):
        """A wedged or gone server comes back as a return code (`tmuxctl.run` never
        raises), and what is lost is the fix rather than the launch — every charter before
        this one shipped with tmux's own binding in place."""
        _seen, patched = self._answering("", code=1)
        with patched:
            self.assertIsNone(commands_frame._menu_button_bind_argv(socket="charter"))

    def test_a_server_that_already_carries_the_wrap_binds_nothing(self):
        """The second frame on a socket. Nothing to do is not a failure: the binding it
        would install is already installed, by the frame that got there first."""
        wrapped = ('bind-key  -T root MouseDown3Pane            if-shell -F -t = '
                   '"#{@charter_panel}" { send-keys -M } { display-menu }')
        _seen, patched = self._answering(wrapped)
        with patched:
            self.assertIsNone(commands_frame._menu_button_bind_argv(socket="charter"))

    def test_the_environment_reaches_the_read(self):
        """The launcher hands its own `_frame_env` to every tmux call it makes, and this
        one is a read against the same server as the writes it is batched with."""
        calls: list[dict] = []

        def fake(action, argv, **kw):
            calls.append(kw)
            return subprocess.CompletedProcess(argv, 0, stdout=_LINE_37C, stderr="")

        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            commands_frame._menu_button_bind_argv(socket="charter", env={"A": "b"})
        self.assertEqual([c.get("env") for c in calls], [{"A": "b"}])


class TheKeyItself(unittest.TestCase):
    """`tmuxctl.CLICK_MENU_KEY`, and where it may and may not appear."""

    def test_the_key_is_the_one_tmux_calls_it(self):
        self.assertEqual(tmuxctl.CLICK_MENU_KEY, "MouseDown3Pane")

    def test_it_is_one_of_the_mouse_keys_a_component_may_not_claim(self):
        """Reserved the moment charter started binding it — `tests/test_a_click_on_a_panel
        _stays_where_it_points.py` holds the refusal itself; this is the membership the
        refusal is built from."""
        self.assertIn(tmuxctl.CLICK_MENU_KEY, tmuxctl.MOUSE_KEYS)

    def test_it_is_not_a_line_in_the_config_file(self):
        """The property that makes an unbalanced brace in an operator's own binding cost
        one key instead of the frame. `conf_text`'s text is `source-file`d whole, and a
        line that fails to parse there takes `mouse`, `history-limit` and the palette's own
        hotkey with it."""
        text = commands_frame.conf_text(hotkey="F2", mouse=True, history_limit=1,
                                        session="fr-1")
        self.assertNotIn(tmuxctl.CLICK_MENU_KEY, text)


if __name__ == "__main__":  # pragma: no cover
    unittest.main()
