"""#810 group B: the refusal branch, on a path whose working half is well tested.

**The cause, in one line: every case drives the working path and asserts what came back, so
a refusal is only ever the branch NOT taken — the fixture is always well-formed enough to
get past the gate.** It is the branch's fallback cluster (*"not five gaps, one fixture that
never lies"*) one level out: there the fixture always wrote a value, here it always passes
the check.

So the remedy is **fixtures that lie**, not tests that assert more. Every case below plants
a plane in a state the existing fixtures cannot produce — a chat with no workspace, a
manifest whose focus workspace came back empty, a pane record that is not a pane id, a
recorded harness this charter cannot launch, a chat id `contain.child` will take and
`chats.ID_RE` will not — and then asserts the thing the branch promises.

**`_attach_after_reopen`'s ladder is first**, which #810 calls the sub-group worth doing
first and it is right: four rungs, each one step further from "where you were", and the
whole point is that **none of them is nowhere**. That is the property no test stated, on
the code path that decides which chat the operator lands on after a restart. The ladder is
also the clearest case of the group's shape — every existing reopen case has an `active`
chat in the focus workspace, so rung 1 answered every time and rungs 2, 3 and 4 were dead
code as far as the suite could tell.
"""

from __future__ import annotations

import os
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, inflight, util
from charter.frame import chats, leave, reopen, state

from tests._isolation import PersonaIso


def _chat(**kw):
    base = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                cwd="/tmp", resume="", transcript="", active=False)
    base.update(kw)
    return reopen.Chat(**base)


def _manifest(*frames, focus="alpha", at=1700000000):
    return reopen.Manifest(at=at, focus=focus, frames=tuple(frames))


def _frame(workspace, *chats_):
    return reopen.Frame(workspace=workspace, chats=tuple(chats_))


def _back(*pairs):
    """`(recorded chat, new fid)` pairs as the `Reopening` list `cmd_reopen` builds."""
    out = []
    for chat, fid in pairs:
        r = commands_frame.Reopening(chat)
        r.fid = fid
        out.append(r)
    return out


_UNSET = object()


def _doomed(**kw):
    base = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                cwd="/tmp", resume="", server=commands_frame.SOCKET, live=True,
                active=False, exit_code=None, closed=False, homeless=False,
                cwd_gone=False)
    base.update(kw)
    return leave.Doomed(**base)


class TheLadderAlwaysLandsSomewhere(PersonaIso):
    """`_attach_after_reopen`'s four rungs, one case each — and one that says the ladder
    has no bottom rung that is `None`.

    Every rung is measured by **which session charter attaches to** and **which pane it
    selects first**, because those two are the whole observable output of this function:
    the operator's terminal ends up on one chat of one tmux session, and the ladder is the
    only thing that decides which.
    """

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())
        self.selected: list[str] = []
        self.attached: list[str] = []

    def _attach(self, m, back) -> int:
        def run(_action, argv, **_kw):
            if "select-window" in argv:
                self.selected.append(argv[-1])
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        def interact(argv):
            self.attached.append(argv[-1])
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        with mock.patch.object(commands_frame.tmuxctl, "run", side_effect=run), \
                mock.patch.object(commands_frame.tmuxctl, "interact",
                                  side_effect=interact):
            return commands_frame._attach_after_reopen(m, back)

    def _pane(self, fid: str, pane: str):
        state.frame_dir(fid, create=True)
        state.record_harness_pane(fid, pane)

    def test_rung_one_is_the_active_chat_of_the_focus_workspace(self):
        m = _manifest(_frame("alpha", _chat(chat="alpha.1"),
                             _chat(chat="alpha.2", active=True)),
                      _frame("beta", _chat(chat="beta.1", workspace="beta", active=True)),
                      focus="alpha")
        back = _back((m.frames[0].chats[0], "alpha.1"),
                     (m.frames[0].chats[1], "alpha.2"),
                     (m.frames[1].chats[0], "beta.1"))
        self._pane("alpha.2", "%22")

        self.assertEqual(self._attach(m, back), 0)

        self.assertEqual(self.selected, ["%22"])
        self.assertEqual(self.attached, ["alpha"])

    def test_rung_two_is_the_first_chat_of_the_focus_workspace(self):
        """The focus workspace came back with nothing marked `active` — the quit ran from
        a workspace whose client had moved, or the mark was lost with a chat that did not
        come back. One step further out: still that workspace, first chat."""
        m = _manifest(_frame("alpha", _chat(chat="alpha.7"), _chat(chat="alpha.8")),
                      _frame("beta", _chat(chat="beta.1", workspace="beta", active=True)),
                      focus="alpha")
        back = _back((m.frames[0].chats[0], "alpha.1"),
                     (m.frames[0].chats[1], "alpha.2"),
                     (m.frames[1].chats[0], "beta.1"))
        self._pane("alpha.1", "%11")

        self.assertEqual(self._attach(m, back), 0)

        self.assertEqual(self.selected, ["%11"])
        self.assertEqual(self.attached, ["alpha"],
                         "the focus workspace, even with nothing active in it")

    def test_rung_three_is_the_active_chat_anywhere(self):
        """The focus workspace did not come back at all — every one of its chats failed to
        relaunch, or the manifest named a workspace that has since been removed. The
        operator still lands on the conversation that had the client."""
        m = _manifest(_frame("beta", _chat(chat="beta.1", workspace="beta"),
                             _chat(chat="beta.2", workspace="beta", active=True)),
                      focus="alpha")
        back = _back((m.frames[0].chats[0], "beta.1"),
                     (m.frames[0].chats[1], "beta.2"))
        self._pane("beta.2", "%42")

        self.assertEqual(self._attach(m, back), 0)

        self.assertEqual(self.selected, ["%42"])
        self.assertEqual(self.attached, ["beta"])

    def test_rung_four_is_the_first_chat_that_came_back_at_all(self):
        """Nothing matches the focus and nothing is active. `back[0]` — and the reason this
        rung exists is the whole property: **none of them is nowhere.** Without it the
        attach would have no session name and the operator would land on whichever window
        tmux happened to make current, which is an answer nobody chose."""
        m = _manifest(_frame("beta", _chat(chat="beta.1", workspace="beta"),
                             _chat(chat="beta.2", workspace="beta")),
                      focus="alpha")
        back = _back((m.frames[0].chats[0], "beta.1"),
                     (m.frames[0].chats[1], "beta.2"))
        self._pane("beta.1", "%51")

        self.assertEqual(self._attach(m, back), 0)

        self.assertEqual(self.selected, ["%51"])
        self.assertEqual(self.attached, ["beta"])

    def test_a_chat_with_no_usable_pane_record_still_attaches(self):
        """The `select-window` is skipped and the attach is not. A pane record charter
        cannot use — never written, or written by a server that has since restarted — costs
        the operator the tab they were on, never the terminal."""
        m = _manifest(_frame("alpha", _chat(chat="alpha.1", active=True)), focus="alpha")
        back = _back((m.frames[0].chats[0], "alpha.1"))
        # No pane recorded at all, then a record that is not a pane id.
        for pane in ("", "not-a-pane"):
            self.selected.clear()
            self.attached.clear()
            if pane:
                self._pane("alpha.1", pane)

            self.assertEqual(self._attach(m, back), 0)

            self.assertEqual(self.selected, [], repr(pane))
            self.assertEqual(self.attached, ["alpha"], repr(pane))

    def test_the_session_is_the_sanitised_workspace_and_not_its_name(self):
        """`workspace_prefix`, and it is not decoration: a workspace called `api.2` reaches
        tmux as `api_2`, because a `-t` with a dot in it is parsed as `window.pane` (#695).
        The ladder picks the chat; this is the name the attach is aimed at."""
        m = _manifest(_frame("api.2", _chat(chat="api_2.1", workspace="api.2",
                                            active=True)), focus="api.2")
        back = _back((m.frames[0].chats[0], "api_2.1"))
        self._pane("api_2.1", "%9")

        self.assertEqual(self._attach(m, back), 0)

        self.assertEqual(self.attached, ["api_2"])

    def test_an_attach_that_fails_is_reported_and_its_code_returned(self):
        m = _manifest(_frame("alpha", _chat(chat="alpha.1", active=True)), focus="alpha")
        back = _back((m.frames[0].chats[0], "alpha.1"))
        reported: list = []

        with mock.patch.object(commands_frame.tmuxctl, "run",
                               return_value=SimpleNamespace(returncode=0, stdout="",
                                                            stderr="")), \
                mock.patch.object(commands_frame.tmuxctl, "interact",
                                  return_value=SimpleNamespace(returncode=3, stdout="",
                                                               stderr="no")), \
                mock.patch.object(commands_frame.tmuxctl, "report_failure",
                                  side_effect=lambda *a: reported.append(a[0])):
            self.assertEqual(commands_frame._attach_after_reopen(m, back), 3)

        self.assertEqual(reported, ["attaching to the reopened frame"])


class AQuitRecordsWhatItCanAndSaysWhatItCouldNot(PersonaIso):
    """`_record_the_plane`'s refusals — the fixture that never had a chat without a
    workspace, never had a pane record charter could not use, and never had a chat whose
    window the server did not report."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def _record(self, doomed, *, focus="alpha", active=(), windows=None, captured=True):
        seen: list = []

        def capture(server, pane_id, dest):
            seen.append((server, pane_id))
            if captured:
                config.write_for(dest, "screen\n")
            return captured

        with mock.patch.object(commands_frame, "_capture_transcript",
                               side_effect=capture):
            kept = commands_frame._record_the_plane(
                doomed, focus=focus, active=set(active),
                windows=windows if windows is not None else {})
        return kept, seen

    def test_a_chat_with_no_workspace_is_stopped_and_not_recorded(self):
        """`if not c.workspace: continue`. Recording it would put a line in the manifest
        that `reopen.read` discards — a record that looks like a promise and is not."""
        kept, _ = self._record([_doomed(chat="alpha.1"), _doomed(chat="lost.1",
                                                                workspace="")])

        self.assertEqual(kept, 1)
        m = reopen.read()
        self.assertEqual([c.chat for c in m.all_chats()], ["alpha.1"])

    def test_every_chat_without_a_workspace_leaves_nothing_to_write(self):
        """The same branch with nothing else in the plan, so the manifest is empty rather
        than the branch merely being outvoted by a sibling."""
        kept, _ = self._record([_doomed(chat="lost.1", workspace="")])

        self.assertEqual(kept, 0)
        self.assertEqual(reopen.read().frames, (),
                         "the manifest lands, and it names nobody")
        with mock.patch.object(util, "err") as said:
            self.assertEqual(commands_frame.cmd_reopen(SimpleNamespace()), 1)
        self.assertEqual(said.call_args[0][0], commands_frame.NOTHING_RECORDED)

    def test_a_pane_record_charter_cannot_use_captures_nothing(self):
        """`_PANE_ID_RE.fullmatch(pane_id)` — a `%N` and nothing else. A record written by
        a server that has since restarted names a pane belonging to somebody else, and
        capturing THAT would put another chat's screen in this one's transcript."""
        state.frame_dir("alpha.1", create=True)
        state.record_harness_pane("alpha.1", "%1")
        good, seen = self._record([_doomed(chat="alpha.1")],
                                  windows={commands_frame.SOCKET: {"alpha.1": None}})
        self.assertEqual(seen, [(commands_frame.SOCKET, "%1")])
        self.assertEqual(reopen.read().all_chats()[0].transcript, "alpha.1.transcript")

        reopen.forget()
        state.record_harness_pane("alpha.1", "not-a-pane")
        _kept, seen = self._record([_doomed(chat="alpha.1")],
                                   windows={commands_frame.SOCKET: {"alpha.1": None}})

        self.assertEqual(seen, [], "nothing was captured")
        self.assertEqual(reopen.read().all_chats()[0].transcript, "",
                         "and the chat still comes back, with nothing to offer")
        self.assertEqual(good, 1)

    def test_a_chat_with_no_pane_recorded_at_all_captures_nothing(self):
        """The `or ""` beside it: `harness_pane` answers `None` for a chat that never
        recorded one, and `None` is not something `_PANE_ID_RE.fullmatch` may be handed."""
        state.frame_dir("alpha.1", create=True)
        self.assertIsNone(state.harness_pane("alpha.1"))

        kept, seen = self._record([_doomed(chat="alpha.1")],
                                  windows={commands_frame.SOCKET: {"alpha.1": None}})

        self.assertEqual(seen, [])
        self.assertEqual(kept, 1)

    def test_a_chat_whose_window_the_server_does_not_report_captures_nothing(self):
        """The third conjunct. A chat whose window is gone has nothing on screen to read,
        and asking tmux to capture from it would be asking about a pane id that a live
        server may since have given to someone else."""
        state.frame_dir("alpha.1", create=True)
        state.record_harness_pane("alpha.1", "%1")

        kept, seen = self._record([_doomed(chat="alpha.1")],
                                  windows={commands_frame.SOCKET: {}})

        self.assertEqual(seen, [])
        self.assertEqual(kept, 1)
        self.assertEqual(reopen.read().all_chats()[0].transcript, "")

    def test_a_chat_with_no_destination_for_its_capture_captures_nothing(self):
        """`dest is not None` — `transcript_path` refuses an id that cannot name a file,
        and a chat id off `os.scandir` is not one charter minted."""
        state.frame_dir("alpha.1", create=True)
        state.record_harness_pane("alpha.1", "%1")
        with mock.patch.object(commands_frame.reopen_state, "transcript_path",
                               return_value=None):
            kept, seen = self._record([_doomed(chat="alpha.1")],
                                      windows={commands_frame.SOCKET: {"alpha.1": None}})

        self.assertEqual(seen, [])
        self.assertEqual(kept, 1)

    def test_a_manifest_that_will_not_land_is_the_one_thing_that_stops_a_quit(self):
        with mock.patch.object(commands_frame.reopen_state, "write", return_value=False):
            kept, _ = self._record([_doomed(chat="alpha.1")])

        self.assertIsNone(kept)


class TheQuitLineNamesTheTrackerOnlyWhenItClearedSomething(PersonaIso):
    """`f" ({pruned} in-flight records cleared)" if pruned else ""` — the clause that is
    absent when it would say nothing. Every existing quit case has an empty tracker, so
    only the empty half was ever rendered."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def _quit_line(self, pruned: int) -> str:
        said: list[str] = []
        with mock.patch.object(commands_frame, "_plane_servers", return_value=()), \
                mock.patch.object(commands_frame, "_plane_live",
                                  return_value=({"alpha.1"}, {}, set())), \
                mock.patch.object(commands_frame.leave, "plan",
                                  return_value=leave.Plan(chats=(_doomed(),),
                                                          focus="alpha")), \
                mock.patch.object(commands_frame, "_record_the_plane", return_value=1), \
                mock.patch.object(commands_frame, "_stop_chats", return_value=1), \
                mock.patch.object(commands_frame, "_warn_about"), \
                mock.patch.object(inflight, "prune_all", return_value=pruned), \
                mock.patch.object(util, "ok", side_effect=said.append):
            self.assertEqual(commands_frame.cmd_quit(SimpleNamespace(chat="alpha.1")), 0)
        return said[0]

    def test_a_tracker_that_cleared_nothing_adds_no_clause(self):
        line = self._quit_line(0)

        self.assertNotIn("in-flight", line)
        self.assertTrue(line.endswith("`charter reopen`"))

    def test_a_tracker_that_cleared_records_says_how_many(self):
        line = self._quit_line(3)

        self.assertIn("(3 in-flight records cleared)", line)


class ClosingOneChatRefusesWhatItCannotName(PersonaIso):
    """`cmd_close`'s gates. The suite drove the working path — a real chat id, open on the
    plane — so every refusal was the branch not taken."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def _close(self, args, *, live=(), plan_chats=None):
        said: list[str] = []
        stopped: list = []
        p = leave.Plan(chats=tuple(plan_chats or ()), focus="")
        with mock.patch.object(commands_frame, "_plane_servers", return_value=()), \
                mock.patch.object(commands_frame, "_plane_live",
                                  return_value=(set(live), {}, set())), \
                mock.patch.object(commands_frame.leave, "plan", return_value=p), \
                mock.patch.object(commands_frame, "_stop_chats",
                                  side_effect=lambda d, **k: (stopped.extend(d),
                                                              len(d))[1]), \
                mock.patch.object(commands_frame, "_warn_about"), \
                mock.patch.object(util, "err", side_effect=said.append), \
                mock.patch.object(util, "ok", side_effect=said.append):
            rc = commands_frame.cmd_close(args)
        return rc, said, stopped

    def test_outside_a_frame_with_no_argument_it_says_where_it_is(self):
        """`if not target:` — the presser's chat is empty AND no `--chat-id` was given."""
        rc, said, stopped = self._close(SimpleNamespace(chat="", chat_id=""))

        self.assertEqual(rc, 1)
        self.assertEqual(stopped, [])
        # The sentence, not merely a refusal: without this gate the empty target falls
        # through to the SHAPE check and is refused there with "'' cannot name a chat",
        # which tells an operator at a prompt outside a frame nothing they can act on.
        self.assertTrue(said)
        self.assertIn("acts on the frame it is run inside", said[0])

    def test_an_argument_of_only_whitespace_is_no_argument(self):
        """The `.strip()`: a `--chat-id` tmux expanded to spaces is not a chat name, and
        without the strip it would pass `if not target` and be refused one gate later with
        the wrong sentence."""
        rc, said, stopped = self._close(SimpleNamespace(chat="", chat_id="   "))

        self.assertEqual(rc, 1)
        self.assertEqual(stopped, [])
        self.assertIn("acts on the frame it is run inside", said[0])

    def test_whitespace_on_either_side_of_a_name_is_not_part_of_it(self):
        """`strip` and not `lstrip`, which is the difference tmux's own expansion can
        produce: `#{@charter_chat}` in a `bind` reaches this as a shell word, and a
        trailing space left on it makes a perfectly good chat id fail `chats.ID_RE` — so
        the chat the operator asked to close is refused as a name instead."""
        state.frame_dir("alpha.1", create=True)
        rc, _said, stopped = self._close(SimpleNamespace(chat="", chat_id=" alpha.1 "),
                                         plan_chats=[_doomed(chat="alpha.1")])

        self.assertEqual(rc, 0)
        self.assertEqual([c.chat for c in stopped], ["alpha.1"])

    def test_an_absent_argument_falls_back_to_the_pressers_own_chat(self):
        """`getattr(args, "chat_id", None) or ""` — the palette's bind supplies no
        `--chat-id` at all, and the chat the key was pressed in is the target."""
        state.frame_dir("alpha.1", create=True)
        rc, said, stopped = self._close(SimpleNamespace(chat="alpha.1"),
                                        plan_chats=[_doomed(chat="alpha.1")])

        self.assertEqual(rc, 0)
        self.assertEqual([c.chat for c in stopped], ["alpha.1"])

    def test_closing_the_chat_you_are_in_says_nothing_on_a_screen_that_is_going(self):
        """`on=fid if fid != target else ""` — `_warn_about` draws its sentence in the chat
        named by `on`, and that pane is about to be killed. Closing a SIBLING keeps the
        sentence, because that screen survives."""
        state.frame_dir("alpha.1", create=True)
        state.frame_dir("alpha.2", create=True)
        seen: list = []
        with mock.patch.object(commands_frame, "_plane_servers", return_value=()), \
                mock.patch.object(commands_frame, "_plane_live",
                                  return_value=(set(), {}, set())), \
                mock.patch.object(commands_frame.leave, "plan",
                                  return_value=leave.Plan(chats=(_doomed(chat="alpha.1"),),
                                                          focus="")), \
                mock.patch.object(commands_frame, "_stop_chats", return_value=1), \
                mock.patch.object(commands_frame, "_warn_about",
                                  side_effect=lambda p, **kw: seen.append(kw["on"])):
            commands_frame.cmd_close(SimpleNamespace(chat="alpha.1", chat_id="alpha.1"))
            commands_frame.cmd_close(SimpleNamespace(chat="alpha.2", chat_id="alpha.1"))

        self.assertEqual(seen, ["", "alpha.2"])


class ForgettingATranscriptTouchesOnlyTheChatItNames(PersonaIso):
    """`_forget_transcript`'s two guards. Every existing case closes a chat the manifest
    names, so "there is no manifest" and "this chat is not in it" were never taken."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def test_a_plane_with_no_manifest_is_left_alone(self):
        self.assertIsNone(reopen.read())

        self.assertIsNone(commands_frame._forget_transcript("alpha.1"))

        self.assertIsNone(reopen.read(), "nothing was written where nothing was")

    def test_a_manifest_that_does_not_name_this_chat_is_not_rewritten(self):
        """`any(c.chat == fid for c in m.all_chats())`, and the assertion has to be about
        the BYTES rather than about the content.

        `_forget_transcript`'s own docstring says what the guard is for: *what goes back is
        the manifest that was read with one chat's entry gone, so a manifest charter could
        not read is left exactly as it was rather than replaced by charter's reading of
        it.* A rewrite that happened anyway would produce byte-identical output for a
        manifest charter itself wrote — `write` is deterministic and `at` is carried over —
        so a case that planted a canonical manifest could not tell the two apart. Measured:
        without this, dropping the `any(...)` conjunct leaves the module green.

        So the manifest on disk is one charter can READ and did not WRITE — same content,
        different formatting, which is what a hand-edited or older-charter manifest looks
        like."""
        canonical = [_frame("alpha", _chat(chat="alpha.1"))]
        reopen.write(canonical, focus="alpha", at=111)
        import json
        payload = json.loads(reopen.path().read_text())
        hand_written = json.dumps(payload, indent=4, sort_keys=False) + "\n"
        self.assertNotEqual(hand_written, reopen.path().read_text(),
                            "or this case cannot tell a rewrite from a no-op")
        reopen.path().write_text(hand_written)
        self.assertIsNotNone(reopen.read(), "and charter must still be able to read it")

        commands_frame._forget_transcript("beta.9")

        self.assertEqual(reopen.path().read_text(), hand_written)

    def test_the_chats_entry_goes_and_the_others_stay(self):
        reopen.write([_frame("alpha", _chat(chat="alpha.1"), _chat(chat="alpha.2")),
                      _frame("beta", _chat(chat="beta.1", workspace="beta"))],
                     focus="alpha", at=222)

        commands_frame._forget_transcript("alpha.1")

        m = reopen.read()
        self.assertEqual([c.chat for c in m.all_chats()], ["alpha.2", "beta.1"])
        self.assertEqual(m.at, 222, "the record is rewritten, not restamped")

    def test_a_frame_left_with_no_chats_goes_with_its_last_one(self):
        """`[f for f in frames if f.chats]` — a workspace whose only chat was closed is not
        a workspace to rebuild, and an empty frame in the manifest would make `charter
        reopen` create a tmux session with nothing in it."""
        reopen.write([_frame("alpha", _chat(chat="alpha.1")),
                      _frame("beta", _chat(chat="beta.1", workspace="beta"))],
                     focus="alpha")

        commands_frame._forget_transcript("alpha.1")

        # The FILE, not `reopen.read`: the reader drops an empty frame on the way back
        # too, so a case that only asked it would stay green with an empty frame on disk —
        # and what a later charter reads is the file.
        import json
        raw = json.loads(reopen.path().read_text())
        self.assertEqual([f["workspace"] for f in raw["frames"]], ["beta"])
        self.assertEqual([f.workspace for f in reopen.read().frames], ["beta"])


class ReopeningOneChatSaysWhatItChanged(PersonaIso):
    """`_reopen_one`'s fallbacks. Every existing case records a harness this charter can
    launch, into a directory that exists, with an id to resume — so the four warnings and
    the two clauses on the success line were only ever the branch not taken."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        # `_reopen_one` does a real `os.chdir` into the recorded directory and restores it
        # in a `finally`. The restore is what this relies on, so the runner is put back
        # explicitly as well — a case that ends inside a directory `PersonaIso` is about to
        # `rmtree` leaves every later `os.getcwd()` in the whole run raising, which is
        # forty errors in nine unrelated modules and nothing pointing back here.
        self.addCleanup(os.chdir, os.getcwd())
        config.private_mkdir(state._root())

    def _reopen(self, chat, *, harness_default=None, rc=0, harness_table=_UNSET):
        said: list[str] = []
        seen: list = []

        def launch(args):
            seen.append(args)
            args.reopening.fid = "alpha.1"
            return rc

        table = ({} if harness_default is None else {"default": harness_default}) \
            if harness_table is _UNSET else harness_table
        with mock.patch.object(config, "HARNESS", table), \
                mock.patch.object(util, "warn", side_effect=said.append), \
                mock.patch.object(util, "info", side_effect=said.append), \
                mock.patch.object(commands_frame, "cmd_launch", side_effect=launch):
            out = commands_frame._reopen_one(chat)
        return out, said, seen

    def test_a_plane_with_no_harness_table_at_all_is_not_a_crash(self):
        """`(config.HARNESS or {}).get("default")` — a plane whose `charter.toml` declares
        no `[harness]` section reads `None`, and `None.get` is the traceback this prevents.
        Every fixture in the suite has a table."""
        out, said, seen = self._reopen(_chat(harness="a-harness-from-2019"),
                                       harness_table=None)

        self.assertIsNone(out)
        self.assertEqual(seen, [])
        self.assertTrue(any("declares no `[harness] default`" in s for s in said))

    def test_a_harness_charter_cannot_launch_falls_back_and_says_so(self):
        out, said, seen = self._reopen(_chat(harness="a-harness-from-2019"),
                                       harness_default="claude")

        self.assertIsNotNone(out)
        self.assertEqual(seen[0].harness, "claude")
        self.assertTrue(any("reopening it under claude" in s for s in said))

    def test_a_directory_that_has_gone_falls_back_to_the_plane_root_and_says_so(self):
        """`if where != c.cwd:` — the warning that only exists when the fallback fired."""
        out, said, seen = self._reopen(_chat(cwd="/nowhere-at-all"))

        self.assertIsNotNone(out)
        self.assertTrue(any("directory" in s and "reopening in" in s for s in said))

    def test_a_directory_that_is_there_says_nothing_about_it(self):
        _out, said, _seen = self._reopen(_chat(cwd=str(config.ROOT)))

        self.assertFalse([s for s in said if "reopening in" in s],
                         "a fallback that did not fire has nothing to report")

    def test_a_chat_with_no_id_is_reported_as_coming_back_empty(self):
        """`leave.RESUMES if rest else 'empty'`. Both halves, because the constant half was
        the only one any case had ever rendered."""
        _out, said, _seen = self._reopen(_chat(resume=""))
        self.assertTrue(any("· empty" in s for s in said))

        _out, said, _seen = self._reopen(_chat(resume="conv-1"))
        self.assertTrue(any(f"· {leave.RESUMES}" in s for s in said))

    def test_a_chat_whose_workspace_is_gone_is_reported_as_missing(self):
        """`" · workspace is missing" if c.workspace and not workspace_dir(...).is_dir()`.
        Both conjuncts: a chat with no workspace at all says nothing (it is `_usable`'s
        business, and this line would name the empty string), and one whose workspace
        directory is there says nothing either."""
        _out, said, _seen = self._reopen(_chat(workspace="ghost"))
        self.assertTrue(any("workspace is missing" in s for s in said))

        from charter import workspace as ws_mod
        config.private_mkdir(ws_mod.workspace_dir("alpha"))
        _out, said, _seen = self._reopen(_chat(workspace="alpha"))
        self.assertFalse([s for s in said if "workspace is missing" in s])

    def test_a_chat_with_no_workspace_hands_the_launcher_none_and_not_an_empty_name(self):
        """`workspace=c.workspace or None` — `cmd_launch` reads `None` as "resolve one" and
        `""` as a workspace called nothing, which is a `-t` target and a directory join."""
        _out, said, seen = self._reopen(_chat(workspace=""))

        self.assertIsNone(seen[0].workspace)
        # And the other conjunct on the report line: a chat that names NO workspace is not
        # a chat whose workspace is missing — `leave.NOT_REOPENED` already said what it is,
        # and this line would name the empty string.
        self.assertFalse([s for s in said if "workspace is missing" in s])

    def test_a_launcher_that_failed_reports_the_chat_as_not_come_back(self):
        out, said, _seen = self._reopen(_chat(), rc=1)

        self.assertIsNone(out)
        self.assertTrue(any("did not come back (launcher returned 1)" in s for s in said))

    def test_a_launcher_that_claimed_no_id_reports_the_same(self):
        """`or not r.fid` — a launch that returned 0 and never claimed an ordinal is a chat
        with nowhere to live, and reporting it as back would put a tab on the bar for a
        frame directory that does not exist."""
        def launch(args):
            return 0                       # never sets `reopening.fid`

        with mock.patch.object(config, "HARNESS", {}), \
                mock.patch.object(util, "warn") as warned, \
                mock.patch.object(commands_frame, "cmd_launch", side_effect=launch):
            self.assertIsNone(commands_frame._reopen_one(_chat()))

        self.assertTrue(any("did not come back" in c[0][0] for c in warned.call_args_list))


class RestoringAChatsRecordsChecksTheNameItWasGiven(PersonaIso):
    """`_restore_recorded_chat`'s two guards, and both are about a value off the manifest —
    a plain file that outlives the process and can be older than the charter reading it."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def test_a_recorded_persona_that_is_not_a_name_is_not_pointed_at(self):
        """`persona_mod.valid_name(rec.persona)`, and it is asked of every falsy value a
        manifest can carry as well as of a hostile one.

        **The `rec.persona and` that used to stand in front of it is gone**, and this case
        is why it could go: `valid_name` is total — `None`, `0` and `[]` off a hand-edited
        or older manifest all answer `False` without raising — so the conjunct could only
        restate what the call was about to say. Both are covered here so that a later
        change to `valid_name` that made it partial reddens rather than crashing a
        relaunch."""
        from charter import persona as persona_mod

        pointer = config.SESSIONS_DIR / "alpha.1.persona"
        for bad in ("", None, 0, [], "../elsewhere", "not a name"):
            commands_frame._restore_recorded_chat(_chat(chat="alpha.9", persona=bad),
                                                  "alpha.1")
            # The pointer FILE, and not `for_session`: that reader refuses a bad name on
            # the way back too, so a case that only asked it would stay green with
            # `../elsewhere` written into the plane's own state directory.
            self.assertFalse(pointer.exists(), repr(bad))
            self.assertIsNone(persona_mod.for_session("alpha.1"), repr(bad))

        self.make_persona("steward")
        commands_frame._restore_recorded_chat(_chat(chat="alpha.9", persona="steward"),
                                              "alpha.1")
        self.assertEqual(persona_mod.for_session("alpha.1"), "steward")

    def test_a_recycled_ordinal_moves_no_transcript_onto_itself(self):
        """`old == new`. `new_chat_id` walks upward from 1 and `reap` frees the ordinals a
        quit's chats held, so a reopen very often gets the SAME id back — and `os.replace`
        of a file onto itself is a no-op on POSIX and not one everywhere."""
        config.write_for(reopen.transcript_path("alpha.1"), "what was on screen\n")

        commands_frame._restore_recorded_chat(_chat(chat="alpha.1"), "alpha.1")

        self.assertEqual(reopen.transcript_path("alpha.1").read_text(),
                         "what was on screen\n")

    def test_a_recorded_id_that_cannot_name_a_transcript_moves_nothing(self):
        """`old is None or new is None` — `transcript_path` refuses an id that is not a
        chat id, and the recorded one comes off the manifest."""
        with mock.patch.object(commands_frame.reopen_state, "transcript_path",
                               side_effect=[None, None]):
            self.assertIsNone(
                commands_frame._restore_recorded_chat(_chat(chat="alpha.9"), "alpha.1"))


class ATranscriptIsOfferedOnlyWhereThereIsOne(PersonaIso):
    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def test_outside_a_frame_it_says_where_it_is(self):
        """`if not fid:` — `cmd_transcript` run with no chat to be about."""
        with mock.patch.object(util, "err") as said:
            rc = commands_frame.cmd_transcript(SimpleNamespace(chat=""))

        self.assertEqual(rc, 1)
        self.assertTrue(said.called)

    def test_a_window_tmux_would_not_open_names_the_file_instead(self):
        """`if opened.returncode != 0:` — the last refusal on the path, and the one whose
        working half every case drove."""
        state.frame_dir("alpha.1", create=True)
        config.write_for(reopen.transcript_path("alpha.1"), "screen\n")
        said: list = []

        with mock.patch.object(commands_frame.shutil, "which", return_value="/usr/bin/less"), \
                mock.patch.object(commands_frame.chats, "pane_of", return_value="%1"), \
                mock.patch.object(commands_frame, "_pane_place",
                                  return_value=("$0", "@0")), \
                mock.patch.object(commands_frame.tmuxctl, "run",
                                  return_value=SimpleNamespace(returncode=1, stdout="",
                                                               stderr="no")), \
                mock.patch.object(commands_frame, "_say_on_screen",
                                  side_effect=lambda fid, msg, **kw: said.append(msg)):
            self.assertEqual(
                commands_frame.cmd_transcript(SimpleNamespace(chat="alpha.1")), 0)

        self.assertTrue(said)
        self.assertIn("would not open a window", said[0])
        self.assertIn(str(reopen.transcript_path("alpha.1")), said[0])


class TheCaptureRefusesAServerThatSaidNothing(PersonaIso):
    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def test_a_capture_of_only_whitespace_is_no_capture(self):
        """`not out.stdout.strip()` — the `.strip()` itself, which `swap-synonym` reaches:
        a pane that answered a screenful of blank lines has nothing on it, and writing that
        as a transcript would offer the operator an empty pager."""
        dest = reopen.transcript_path("alpha.1")
        answer = SimpleNamespace(returncode=0, stdout="   \n\n  \n", stderr="")

        with mock.patch.object(commands_frame.tmuxctl, "run", return_value=answer):
            self.assertFalse(commands_frame._capture_transcript(
                commands_frame.SOCKET, "%1", dest))

        self.assertFalse(dest.exists())

    def test_a_cut_that_lands_inside_a_character_drops_it_rather_than_mangling_it(self):
        """`.decode("utf-8", "ignore")`, and **this one is not on #810's list.**

        It was found by re-running the sweep's own operators over these functions rather
        than working the issue's survivor list, which is what its author asked for: *a
        survivor list is a union across runs, not a snapshot.*

        The byte cut is taken from the END, so the only edge it can land inside is the
        LEADING one — and every existing case builds its capture out of characters whose
        byte length divides the cap evenly, so the cut always landed on a boundary and the
        handler was never read. (`str.decode` looks an error handler up lazily, exactly
        like `str.encode`: a re-tuned name is a `LookupError` only when a decode actually
        fails.)

        This capture is one byte off that boundary, so the slice begins on a continuation
        byte. What `ignore` buys is that the partial character is DROPPED rather than
        written as a `U+FFFD` the pager would show as noise — which is the sentence the
        comment above the line already makes and nothing measured.
        """
        cap = commands_frame._TRANSCRIPT_BYTES
        text = "é" * cap + "x"          # 2·cap + 1 bytes, so the cut lands mid-character
        raw = text.encode("utf-8")
        self.assertTrue(0x80 <= raw[len(raw) - cap] < 0xC0,
                        "or this case is not about a cut inside a character")
        dest = reopen.transcript_path("alpha.1")
        answer = SimpleNamespace(returncode=0, stdout=text, stderr="")

        with mock.patch.object(commands_frame.tmuxctl, "run", return_value=answer):
            self.assertTrue(commands_frame._capture_transcript(
                commands_frame.SOCKET, "%1", dest))

        got = dest.read_text()          # would raise if half a character were written
        self.assertNotIn("\ufffd", got, "dropped, not replaced with a noise glyph")
        self.assertTrue(got.startswith("é"))
        self.assertTrue(got.endswith("x"))
        self.assertLessEqual(len(dest.read_bytes()), cap)

    def test_a_capture_with_something_in_it_keeps_its_surrounding_blank_lines(self):
        """The other half of the same call, and the reason it is `strip()` on the TEST and
        not on the text: what is written is `out.stdout`, whitespace and all."""
        dest = reopen.transcript_path("alpha.1")
        answer = SimpleNamespace(returncode=0, stdout="\n  hello\n\n", stderr="")

        with mock.patch.object(commands_frame.tmuxctl, "run", return_value=answer):
            self.assertTrue(commands_frame._capture_transcript(
                commands_frame.SOCKET, "%1", dest))

        self.assertEqual(dest.read_text(), "\n  hello\n\n")


class ThePlanReadsAPlaneThatIsNotTidy(PersonaIso):
    """`frame/leave.py`'s refusals. The quit fixture always planted well-formed chat
    directories in one workspace, so every one of these was the branch not taken."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def test_the_workspace_list_skips_a_chat_that_names_none(self):
        """`if c.workspace and c.workspace not in seen` — two conjuncts. A chat with no
        workspace must not put an empty name in the summary's count, and a second chat of a
        workspace already listed must not put it there twice."""
        p = leave.Plan(chats=(_doomed(chat="alpha.1", workspace="alpha"),
                              _doomed(chat="alpha.2", workspace="alpha"),
                              _doomed(chat="lost.1", workspace=""),
                              _doomed(chat="beta.1", workspace="beta")), focus="alpha")

        self.assertEqual(p.workspaces(), ("alpha", "beta"))

    def test_the_scan_skips_a_name_that_is_not_a_chat(self):
        """`chats.is_chat(n)` — the frame root also holds directories an older charter
        made, whose ids end in a launcher pid. `plane_chats` is Stage 5a's discriminator
        asked rather than re-derived."""
        for name in ("alpha.1", "alpha.2", "oldstyle-4242"):
            state.frame_dir(name, create=True)

        self.assertEqual(leave.plane_chats(), ["alpha.1", "alpha.2"])

    def test_the_scan_skips_a_loose_file_that_is_shaped_like_a_chat(self):
        """`e.is_dir()` beside it, and #733's own reason: the frame root holds the
        manifest, the transcripts and whatever a half-finished `os.replace` left behind,
        and a loose FILE called `api.2` is a name that would reach a tmux target."""
        state.frame_dir("alpha.1", create=True)
        config.write_for(state._root() / "alpha.2", "not a chat\n")

        self.assertEqual(leave.plane_chats(), ["alpha.1"])

    def test_a_chat_that_names_no_workspace_sorts_last(self):
        """`(0, ws) if ws else (1, "")` — the migration and truncation case, and a nameless
        group at the end reads as the leftovers it is rather than as a workspace called
        "". Both halves, because every existing fixture recorded a workspace."""
        # The recorded workspaces deliberately DISAGREE with the id prefixes, because
        # `chats._order` sorts by ordinal and then by id — so a fixture whose workspaces
        # happen to sort the same way as its names cannot tell the grouping from the
        # ordering, and the `(1, "")` half of this expression would decide nothing.
        for name in ("alpha.1", "zeta.2", "lost.3"):
            state.frame_dir(name, create=True)
        state.record_workspace("alpha.1", "zeta")
        state.record_workspace("zeta.2", "alpha")

        # `chats._order` alone would answer `alpha.1, zeta.2, lost.3` — ordinal first.
        # Grouped by the recorded WORKSPACE it is `zeta.2` (workspace `alpha`) first, and
        # the chat that names no workspace still last. Both halves of `(0, ws) if ws else
        # (1, "")` decide something here, which is what a fixture whose workspaces agree
        # with its ordinals cannot show.
        self.assertEqual(leave.plane_chats(), ["zeta.2", "alpha.1", "lost.3"])

    def test_a_plane_with_nothing_to_stop_summarises_as_nothing_open(self):
        """`if not doomed: return NOTHING_OPEN` — `summary`'s first line, reached when
        every chat on the plane is already closed or already ended."""
        self.assertEqual(leave.summary(leave.Plan(chats=(), focus="")),
                         leave.NOTHING_OPEN)
        # `stopping` keeps everything the server has not reported ABSENT, so the branch is
        # reached by a chat tmux says is gone — not by one marked closed, which is still
        # attempted (see `stopping`'s own docstring).
        gone = _doomed(chat="alpha.1", live=False)
        self.assertEqual(leave.stopping(leave.Plan(chats=(gone,), focus="")), ())
        self.assertEqual(leave.summary(leave.Plan(chats=(gone,), focus="")),
                         leave.NOTHING_OPEN)

    def test_a_chat_whose_harness_charter_forgot_is_titled_by_its_id_alone(self):
        """`f"{c.chat} · {c.harness}" if c.harness else c.chat` — the migration case, and
        the else half no fixture had ever rendered. A trailing ` · ` would read as a
        truncated line rather than as a chat charter knows nothing more about."""
        self.assertEqual(leave.title(_doomed(chat="alpha.1", harness="")), "alpha.1")
        self.assertEqual(leave.title(_doomed(chat="alpha.1", harness="claude-code")),
                         "alpha.1 · claude-code")


class ANameThatCannotBeAChatHasNoTranscript(PersonaIso):
    """`reopen.transcript_path`'s guard, which #810 calls out separately and correctly:
    it is genuinely masked by `contain.child`'s own refusal, so pinning it means finding a
    name `segment_ok` admits and `chats.ID_RE` does not.

    There is one, and it is not exotic: **a space.** `segment_ok` is a question about
    separators, NUL and rootedness, so `alpha 1` is a perfectly good directory entry — and
    `ID_RE` is `[A-Za-z0-9._-]+`, which refuses it. What the guard adds on top of the path
    containment is the ALPHABET, and the value it bounds is on its way to a `less` argv in
    a tmux `new-window`.
    """

    def test_a_name_the_path_guard_admits_and_the_alphabet_does_not(self):
        from charter import contain

        for name in ("alpha 1", "alpha\t1", "café.1", "alpha*1", "alpha|1"):
            self.assertTrue(contain.segment_ok(f"{name}{reopen.TRANSCRIPT_SUFFIX}"),
                            f"{name}: the path guard admits this, so it cannot be what "
                            f"refuses it")
            self.assertIsNone(reopen.transcript_path(name), name)

    def test_a_missing_id_is_refused_rather_than_handed_to_the_regex(self):
        """`fid or ""` — `ID_RE.fullmatch(None)` is a `TypeError`, and the id comes off a
        manifest field a `null` would satisfy."""
        self.assertIsNone(reopen.transcript_path(None))
        self.assertIsNone(reopen.transcript_path(""))

    def test_a_name_both_guards_admit_gets_its_path(self):
        got = reopen.transcript_path("alpha.1")

        self.assertIsNotNone(got)
        self.assertEqual(got.name, "alpha.1.transcript")


class AChatIdCharterCannotResolveWritesAndReadsNothing(PersonaIso):
    """`state.record_closed` and `state.was_closed`'s `if d is None` guards.

    `frame_dir` resolves through `contain.child` and answers `None` for a name it refuses —
    and `cmd_close`'s target can be a `--chat-id` off a tmux binding. Every existing case
    hands these two an id charter minted, so neither guard was ever entered.
    """

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def test_a_name_that_cannot_be_a_directory_writes_no_marker(self):
        before = sorted(p.name for p in state._root().iterdir())

        for bad in ("../elsewhere", "", "a/b", "."):
            self.assertIsNone(state.record_closed(bad), repr(bad))

        self.assertEqual(sorted(p.name for p in state._root().iterdir()), before,
                         "nothing was created outside, and nothing inside either")

    def test_a_name_that_cannot_be_a_directory_records_no_cwd(self):
        """`state.record_cwd`'s own `if d is None` — **also not on #810's list**, and found
        the same way: the issue named this function's `except OSError` and not the guard
        two lines above it. The value is a chat id off the manifest, and what it records is
        the directory a later reopen will `os.chdir` into."""
        before = sorted(p.name for p in state._root().iterdir())

        for bad in ("../elsewhere", "", "a/b", "."):
            self.assertIsNone(state.record_cwd(bad, "/some/where"), repr(bad))
            self.assertIsNone(state.chat_cwd(bad), repr(bad))

        self.assertEqual(sorted(p.name for p in state._root().iterdir()), before)

    def test_a_name_that_cannot_be_a_directory_reads_as_not_closed(self):
        for bad in ("../elsewhere", "", "a/b", "."):
            self.assertFalse(state.was_closed(bad), repr(bad))

    def test_a_name_that_can_be_one_round_trips(self):
        state.frame_dir("alpha.1", create=True)
        self.assertFalse(state.was_closed("alpha.1"))

        state.record_closed("alpha.1")

        self.assertTrue(state.was_closed("alpha.1"))


class TheLaunchHintCountsOnlyWhatCanResume(PersonaIso):
    """`_say_the_plane_is_recorded` — the line `cmd_launch` prints when the chat it just opened is
    named in a manifest nobody has reopened. Every existing case records one chat with an
    id, so `n` and `back` were the same number and the comprehension's filter decided
    nothing."""

    def setUp(self):
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        config.private_mkdir(state._root())

    def _hint(self, *chats_) -> list[str]:
        reopen.write([_frame("alpha", *chats_)], focus="alpha")
        said: list[str] = []
        with mock.patch.object(util, "info", side_effect=said.append):
            commands_frame._say_the_plane_is_recorded(chats_[0].chat)
        return said

    def test_the_second_number_is_how_many_have_a_conversation(self):
        said = self._hint(_chat(chat="alpha.1", resume="conv-1"),
                          _chat(chat="alpha.2", resume=""),
                          _chat(chat="alpha.3", resume="conv-3"))

        self.assertTrue(said)
        self.assertIn("3 chat(s) recorded, 2 with a conversation to resume", said[0])

    def test_a_chat_the_manifest_does_not_name_is_said_nothing_about(self):
        reopen.write([_frame("alpha", _chat(chat="alpha.1"))], focus="alpha")
        with mock.patch.object(util, "info") as said:
            commands_frame._say_the_plane_is_recorded("beta.9")

        said.assert_not_called()


class TheQuitSummaryCountsOnlyWhatCanResume(PersonaIso):
    """`len([c for c in doomed if c.resume])` and the `any` beside it — the two places the
    summary counts, both of which every fixture answered the same way."""

    def test_the_count_is_of_the_chats_that_have_an_id(self):
        p = leave.Plan(chats=(_doomed(chat="alpha.1", resume="conv-1"),
                              _doomed(chat="alpha.2", resume=""),
                              _doomed(chat="alpha.3", resume="conv-3")), focus="alpha")

        self.assertIn("2 of 3 can resume the conversation", leave.summary(p))

    def test_a_plane_where_nothing_can_resume_says_zero(self):
        p = leave.Plan(chats=(_doomed(chat="alpha.1", resume=""),), focus="alpha")

        self.assertIn("0 of 1 can resume the conversation", leave.summary(p))


if __name__ == "__main__":
    unittest.main()
