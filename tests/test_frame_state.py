"""The frame's own state: who it is, when it last changed, how it ended.

Per frame rather than global, because two frames may run at once (one session each, named
by workspace and pid) and a shared version file would make each frame's panels redraw for
the other's activity.
"""

from __future__ import annotations

import os
import subprocess
import sys
import time
import unittest
from unittest import mock

from tests._isolation import PersonaIso
from charter.frame import state
from tests._tmuxsocket import OPERATOR_SOCKET


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped. Preferred over a large made-up
    number, which is a guess about the machine rather than a fact about it — and since
    #383 the number at the end of a frame id is asked about rather than ignored, a made-up
    one could name somebody else's live process and quietly change what a test proves."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


def _claimed_by_a_departed_launcher(workspace: str) -> str:
    """A chat claimed exactly as a launcher claims one, by a launcher that has exited.

    `state.new_chat_id` — the production allocator, marker and all — with the one fact
    these tests are about stated rather than inherited: **which process made the claim.**
    Since #685 the claim carries its launcher's pid, so a chat this test process claims
    for itself is one this test process keeps alive; that is the fix, and it is exactly
    what makes an unstated pid the wrong fixture for "the launcher is gone".

    `_a_dead_pid`'s own argument, one layer along: a real pid that has exited, never a
    large made-up number that could name somebody else's live process and quietly change
    what a test proves.
    """
    with mock.patch.object(os, "getpid", _a_dead_pid):
        return state.new_chat_id(workspace)


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


class ChatIdIsAllocated(PersonaIso, unittest.TestCase):
    """`state.new_chat_id` — the ordinal is CLAIMED, not computed.

    The operator's own sketch was `{workspace}-{some-hash}`, and both halves of it fail.
    A hash of the only inputs available at creation is a hash of a counter in disguise,
    and it collides silently into a shared `.charter/frame/<fid>/` where one chat's
    `session`, `panes` and `version` overwrite the other's. A `-{ordinal}` tail fails
    worse — see :class:`TheDotIsTheVersionDiscriminator` for the measurement.
    """

    def test_the_first_chat_of_a_workspace_is_one(self):
        self.assertEqual(state.new_chat_id("api"), "api.1")

    def test_the_next_one_is_two(self):
        self.assertEqual(state.new_chat_id("api"), "api.1")
        self.assertEqual(state.new_chat_id("api"), "api.2")

    def test_the_claim_is_the_directory(self):
        """The `mkdir` IS the allocation, so the id comes back with its directory already
        on disk at 0700 — charter's mode, not the umask's (#470/#505). A scan-then-create
        allocator would return a name nothing had claimed yet."""
        fid = state.new_chat_id("api")
        d = state.frame_dir(fid)
        self.assertTrue(d.is_dir())
        self.assertEqual(oct(d.stat().st_mode & 0o777), oct(0o700))

    def test_two_allocators_racing_one_workspace_never_agree(self):
        """The property a scan cannot give. Twenty threads on one barrier, all asking for
        the same workspace: `mkdir` is one syscall and the kernel picks, so each gets its
        own ordinal. Read `max + 1` off a directory listing instead and two racers get
        the same answer, and the loser silently adopts the winner's frame directory —
        which is exactly the silent collision a hash was rejected for."""
        import threading
        n = 20
        barrier = threading.Barrier(n)
        got: list[str | None] = []
        lock = threading.Lock()

        def claim():
            barrier.wait()
            fid = state.new_chat_id("race")
            with lock:
                got.append(fid)

        threads = [threading.Thread(target=claim) for _ in range(n)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        self.assertEqual(len(got), n)
        self.assertNotIn(None, got, "an allocator gave up while ordinals were free")
        self.assertEqual(len(set(got)), n, f"two allocators returned one id: {got}")
        self.assertEqual(sorted(got), sorted(f"race.{i}" for i in range(1, n + 1)))

    def test_a_workspace_spelled_like_a_chat_id_does_not_collide_with_one(self):
        """Workspace `api.2` and chat 2 of workspace `api` are two different names, and
        the allocator has to keep them that way. Since #695 it keeps them apart by not
        minting the dot at all: `api.2`'s chats are `api_2.1`, `api_2.2`, so the only dot
        in a chat id is the ordinal separator and nothing has to be told which one it is.

        Pinned as literals, because the point is which strings come out — and because the
        prefix is a tmux SESSION name, where a dot is a target separator on 3.7c and is
        rewritten to `_` outright on 3.2. Charter now spells what 3.2 was going to spell
        anyway, on both."""
        self.assertEqual(state.new_chat_id("api"), "api.1")
        self.assertEqual(state.new_chat_id("api"), "api.2")
        self.assertEqual(state.new_chat_id("api.2"), "api_2.1")
        self.assertEqual(state.new_chat_id("api"), "api.3")
        self.assertNotEqual(state.workspace_prefix("api.2"), "api.2",
                            "the identifier charter derives still carries a dot, which is "
                            "a tmux target separator")

    def test_a_taken_ordinal_is_skipped_rather_than_adopted(self):
        """A directory already holding another chat's record is not an id to hand out.
        `config.private_mkdir` would have swallowed the `FileExistsError` (#331) and
        returned it — idempotence being exactly wrong for an allocator — so the new chat
        would have inherited that record's `exit`, `density`, `panes` and `session`."""
        state.record_exit("api.1", 99)
        self.assertEqual(state.new_chat_id("api"), "api.2")
        self.assertEqual(state.exit_code("api.1"), 99,
                         "the allocator wrote in a directory it did not claim")

    def test_a_hostile_workspace_name_cannot_escape_the_frame_root(self):
        """The id becomes a directory name, so the same sanitisation `frame_id` does."""
        fid = state.new_chat_id("../../etc")
        self.assertNotIn("/", fid)
        self.assertNotIn("..", fid)
        self.assertTrue(state.frame_dir(fid).is_dir())

    def test_a_workspace_name_that_ends_in_a_dot_does_not_double_it(self):
        """`workspace_prefix` strips BOTH ends, and the trailing one is what this is
        about: `api.` would otherwise mint `api..1`, which reads as chat 1 of a workspace
        called `api.` and is not a thing anyone can say out loud. Stripping the trailing
        end is also what stops the prefix ever being `.` or `..`, the two names
        `contain.segment_ok` refuses."""
        self.assertEqual(state.workspace_prefix("api."), "api")
        self.assertEqual(state.workspace_prefix("api-"), "api")
        self.assertEqual(state.workspace_prefix("api_"), "api")
        self.assertEqual(state.workspace_prefix("_api"), "api",
                         "the leading end too, so `_launcher_pid` still finds a head")
        self.assertEqual(state.workspace_prefix("a.p-i"), "a_p-i",
                         "and only at the ENDS: `-` and `_` are ordinary inside "
                         "a name, or `harness-wrapper` would come back as `harness`")
        self.assertEqual(state.new_chat_id("api."), "api.1")
        self.assertEqual(state.workspace_prefix("..."), "frame",
                         "a name that strips to nothing falls back, or `_launcher_pid` "
                         "would read a head-less name")

    def test_an_id_mkdir_will_not_take_is_a_refusal_rather_than_a_raise(self):
        """`contain.child` bounds shape, not length, so a workspace name thousands of
        characters long passes it and then hits `ENAMETOOLONG`. A launch has to be told
        it cannot open a chat, not handed an exception."""
        self.assertIsNone(state.new_chat_id("x" * 5000))

    def test_giving_up_is_bounded_rather_than_a_spin(self):
        """The loop has a ceiling, and it answers `None` at it. Asserted by shrinking the
        ceiling rather than by making ten thousand directories."""
        with mock.patch.object(state, "_CHAT_ORDINAL_MAX", 2):
            self.assertEqual(state.new_chat_id("api"), "api.1")
            self.assertEqual(state.new_chat_id("api"), "api.2")
            self.assertIsNone(state.new_chat_id("api"))

    def test_the_ceiling_is_the_number_it_was_argued_at(self):
        """The test above shrinks it, so it cannot also say what it is. Ten thousand is
        the number: the scan costs one `mkdir` per taken ordinal and a plane holds tens of
        frame directories, so it is far past any real plane and small enough that giving
        up is a refusal a caller can report rather than a spin nobody can see. Moving it
        needs that argument re-made, which is what this literal asks for."""
        self.assertEqual(state._CHAT_ORDINAL_MAX, 10_000)

    def test_a_frame_root_that_cannot_be_made_is_a_refusal(self):
        """The first thing that can fail, and it fails before any ordinal is tried. A
        launch has to be told it cannot open a chat — not handed an `OSError` out of a
        function this module promises never raises."""
        with mock.patch.object(state.config, "private_mkdir",
                               side_effect=OSError(28, "No space left on device")):
            self.assertIsNone(state.new_chat_id("api"))

    def test_a_filesystem_that_refuses_the_claim_is_a_refusal_too(self):
        """The second, and it is deliberately NOT retried at `n+1`: a full filesystem or a
        permission this plane does not have does not get better one ordinal along, and
        looping over it would spend `_CHAT_ORDINAL_MAX` syscalls to answer the same
        `None`. `FileExistsError` is the one `OSError` that DOES mean "try `n+1`", and
        `test_a_taken_ordinal_is_skipped_rather_than_adopted` is what keeps the two
        apart."""
        with mock.patch.object(state.config, "claim_private_dir",
                               side_effect=OSError(13, "Permission denied")) as claim:
            self.assertIsNone(state.new_chat_id("api"))
        self.assertEqual(claim.call_count, 1,
                         "a failure that cannot get better was retried to the ceiling")


class TheDotIsTheVersionDiscriminator(PersonaIso, unittest.TestCase):
    """Why `{workspace}.{n}` and not `{workspace}-{n}`, measured rather than argued.

    `_launcher_pid` reads a `-<digits>` tail as a launcher pid, and `reap` keeps any
    directory whose launcher is still running. Pid 1 is `launchd`/`init` and pid 2 is a
    kernel thread on every Unix, so a `-{ordinal}` id would make every dead chat look
    live forever — and `reap` is the only thing bounding `.charter/frame/`.
    """

    def test_a_dash_ordinal_would_read_as_a_live_pid(self):
        """The control, and the reason the dot is not cosmetic. Not a claim about the
        design — a measurement of the function the design had to route around."""
        self.assertEqual(state._launcher_pid("myws-2"), 2)
        self.assertEqual(state._launcher_pid("myws-1"), 1)
        self.assertTrue(state._launcher_is_alive(1),
                        "pid 1 is init — if this is ever false the measurement above "
                        "stops meaning what this class says it means")

    def test_a_chat_id_carries_no_pid_at_all(self):
        for name in ("api.3", "harness-wrapper.3", "a-b.1", "api.2.1"):
            with self.subTest(name=name):
                self.assertIsNone(state._launcher_pid(name))

    def test_an_old_frame_id_still_parses_and_still_reports_liveness_by_pid(self):
        """No migration ran, so a frame launched by a charter that predates chats keeps
        the rule it was launched under."""
        old = state.frame_id("legacy", os.getpid())
        state.record_server(old, "charter")
        state.record_harness_pane(old, "%3")
        self.assertEqual(state._launcher_pid(old), os.getpid())
        self.assertTrue(state.is_live(old, pane="%3"))
        self.assertEqual(state.reap(set(), server="charter"), [],
                         "an old frame whose launcher is alive was reaped")

    def test_an_old_frame_whose_launcher_died_is_still_reaped_by_the_old_rule(self):
        old = state.frame_id("legacy", _a_dead_pid())
        state.bump(old)
        self.assertEqual(state.reap(set(), server="charter"), [old])

    def test_a_chat_is_kept_by_the_liveness_list_alone(self):
        """The whole of a chat's bounding rule once its launcher is gone: nothing in the
        NAME abstains in its favour, so the set `reap` is handed decides. `_live_chats`
        is what fills it."""
        chat = _claimed_by_a_departed_launcher("api")
        state.bump(chat)
        self.assertEqual(state.reap({chat}, server="charter"), [])
        self.assertTrue(state.frame_dir(chat).exists())

    def test_a_chat_whose_window_is_gone_is_removed_with_no_launcher_process_alive(self):
        """The other half, and the one the dash would have broken: nothing in the name
        can keep this directory, so an absent chat id really does reap.

        The launcher is a departed one (#685) rather than this test process, and that is
        the difference between measuring the dot and measuring the claim: a chat whose
        launcher is still running is kept on purpose now, so a fixture that left the pid
        unstated would pass this file's own fix off as a regression."""
        chat = _claimed_by_a_departed_launcher("api")
        state.bump(chat)
        self.assertEqual(state.reap(set(), server="charter"), [chat])
        self.assertFalse(state.frame_dir(chat).exists())

    def test_an_old_frame_answers_live_with_no_pane_offered_at_all(self):
        """`pane` is optional because `reap`'s question is the frame's EXISTENCE, not any
        process's membership of it — `None` means "do not ask", never "assume no". Drop
        the `pane is not None` half of that check and an old frame with a live launcher
        reads as dead, which for `statusline` means a frame that never suppresses."""
        old = state.frame_id("legacy", os.getpid())
        state.record_server(old, "charter")
        state.record_harness_pane(old, "%3")
        self.assertTrue(state.is_live(old),
                        "no pane was offered, so there was nothing to disagree with")
        self.assertTrue(state.is_live(old, pane="%3"))
        self.assertFalse(state.is_live(old, pane="%4"))

    def test_a_chats_liveness_is_the_harness_pane_rather_than_a_pid(self):
        """`is_live` runs on Claude Code's own repaint path, so it may not spawn a tmux
        subprocess. For a chat the record of which pane the LAUNCHER started the harness
        in is the evidence: a process running in that pane is inside a window that still
        exists."""
        chat = state.new_chat_id("api")
        state.record_server(chat, "charter")
        state.record_harness_pane(chat, "%4")
        self.assertTrue(state.is_live(chat, pane="%4"))
        self.assertFalse(state.is_live(chat, pane="%5"),
                         "a process that merely inherited this chat's id is not its "
                         "harness")
        self.assertFalse(state.is_live(chat),
                         "with no pane offered there is nothing here that can answer "
                         "for a chat, and a claim with no evidence is the wrong answer")

    def test_a_chat_with_no_server_marker_is_not_live(self):
        """The marker is what says a LAUNCHER made this directory rather than a stray
        `$CHARTER_SESSION_ID` and the first hook that fired for it."""
        chat = state.new_chat_id("api")
        state.record_harness_pane(chat, "%4")
        self.assertFalse(state.is_live(chat, pane="%4"))


class AClaimSurvivesASiblingsReap(PersonaIso, unittest.TestCase):
    """#685: winning the `mkdir` is only half of "two allocators cannot both win".

    The other half is that the winner's directory survives long enough to become a chat.
    Between `new_chat_id` returning and `new-window` there are hundreds of milliseconds —
    `_spawn_gather`, `_frame_env`, `record_identity`, the tmux.conf write — during which
    the claim had a window in no list, no `server` marker, and (Stage 5a, on purpose) no
    launcher pid in its name. Every one of `reap`'s rules said dead, and every launch runs
    a reap immediately before its own claim, so the loser of the race deleted the winner's
    directory and then claimed the same ordinal.
    """

    def test_a_sibling_launch_does_not_un_claim_an_ordinal_just_handed_out(self):
        """The reported reproduction, kept in its reported order: claim, a sibling's
        reap, claim again."""
        first = state.new_chat_id("api")
        self.assertEqual(state.reap(set(), server="charter"), [],
                         "a sibling's reap deleted a directory this process had just "
                         "claimed and was still launching into")
        second = state.new_chat_id("api")
        self.assertNotEqual(first, second,
                            "two live chats were handed one state directory")

    def test_the_two_chats_keep_two_pane_records_rather_than_one(self):
        """What the collision COST, rather than that it happened. One directory means one
        `panes` map, one `exit` file and one harness pane — so a switch aims at whichever
        launcher wrote last, and the roster shows one chat where two are running."""
        first = state.new_chat_id("api")
        state.reap(set(), server="charter")
        second = state.new_chat_id("api")
        state.record_harness_pane(first, "%11")
        state.record_harness_pane(second, "%22")
        self.assertEqual(state.harness_pane(first), "%11")
        self.assertEqual(state.harness_pane(second), "%22")

    def test_a_chats_exit_code_survives_a_reap_that_beats_its_own_launcher(self):
        """#383's protection, which Stage 5a's id shape had silently spent for chats.

        `Reap.test_a_sibling_exit_code_survives_a_reap_that_beats_its_own_launcher` is the
        same property for a `{workspace}-{pid}` frame, where the NAME carried the pid. A
        chat's does not, so without the claim marker a chat whose harness had just exited
        — absent from the live list, its launcher one line from `exit_code` — lost the
        number it was about to read, and `cmd_launch` returned 0 for a harness that
        failed.
        """
        chat = state.new_chat_id("api")
        state.record_exit(chat, 42)
        state.reap({"some-other-chat"}, server="charter")
        self.assertEqual(state.exit_code(chat), 42,
                         "reap deleted a live launcher's chat directory, and with it the "
                         "exit code that launcher had not read yet")

    def test_a_claim_with_nothing_in_it_yet_is_kept(self):
        """The one window the marker itself cannot cover, and why the marker has to be the
        FIRST byte written into a claimed directory rather than merely an early one.

        Between `claim_private_dir`'s `mkdir` and `_record_claim`'s write there is a
        directory `reap` can see and no pid it can read. An empty frame directory is
        therefore a claim caught between two syscalls — never a frame, since every frame
        that has run holds `server`, `workspace` and `version` at minimum.
        """
        d = state.frame_dir("api.7", create=True)
        self.assertEqual(list(d.iterdir()), [], "the fixture is not the empty case")
        self.assertEqual(state.reap(set(), server="charter"), [])
        self.assertTrue(d.is_dir(), "a claim was deleted between its two syscalls")

    def test_a_claim_a_launcher_no_longer_holds_is_reaped_like_any_other(self):
        """The keep-rule is the launcher's liveness and not the marker's presence, which
        is what stops #685's fix from making `.charter/frame/` unbounded — the failure the
        dot in the id exists to prevent, arriving through the file instead."""
        chat = _claimed_by_a_departed_launcher("api")
        self.assertEqual(state.reap(set(), server="charter"), [chat])

    @unittest.skipIf(os.name != "posix" or os.geteuid() == 0,
                     "root reads a mode-000 directory anyway, so there is no unlistable "
                     "directory here to measure against")
    def test_a_directory_that_cannot_be_listed_is_kept_rather_than_raised_over(self):
        """`reap` runs on the launch path and may not raise there, and asking whether a
        directory is empty is the one question in it that opens a directory rather than a
        file. Every other reading here already answers `None` for a path it cannot read;
        this one had to be given the same posture explicitly.

        Kept rather than removed, on the side every unreadable answer in this module falls
        on: `rmtree` would not have emptied it either, so deleting on no evidence would be
        a report of work that did not happen."""
        chat = _claimed_by_a_departed_launcher("api")
        d = state.frame_dir(chat)
        state.bump(chat)
        d.chmod(0o000)
        self.addCleanup(d.chmod, 0o700)
        self.assertEqual(state.reap(set(), server="charter"), [])   # must not raise
        self.assertTrue(d.is_dir())

    def test_the_claim_is_a_plain_file_inside_the_directory_it_claimed(self):
        """Both halves of where the marker goes, because both are load-bearing.

        INSIDE the claimed directory, so `reap` removing that directory removes the marker
        with it and there is nothing outside the frame root to keep in step — the property
        that makes this "no new file to keep in sync" rather than one more thing to reap.

        And the FIRST thing in it, which is what makes the empty-directory rule and the
        pid rule meet with no gap between them: a frame directory holding anything at all
        is one whose claimer has already said who it is.

        Named plainly beside `server` and `workspace` rather than as a dotfile:
        everything under a frame's directory is charter's own bookkeeping and none of it
        hides from `ls`.
        """
        chat = state.new_chat_id("api")
        d = state.frame_dir(chat)
        self.assertEqual([p.name for p in d.iterdir()], ["launcher"],
                         "the claim is not the only thing in a just-claimed directory, "
                         "so `reap`'s empty-directory rule and its pid rule no longer "
                         "meet")
        self.assertEqual((d / "launcher").read_text().strip(), str(os.getpid()))

    def test_a_launcher_that_has_finished_gives_the_claim_up(self):
        """`state.clear_claim` — the half that keeps this fix from making
        `.charter/frame/` unbounded. The marker is held for the LAUNCH, not for the life
        of the process, and a launcher that has read its harness's exit code is done with
        it: without this its own closing reap would be refused by a pid that is,
        necessarily, still alive."""
        chat = state.new_chat_id("api")
        state.bump(chat)
        self.assertEqual(state.reap(set(), server="charter"), [],
                         "the claim did not keep the directory, so this test cannot say "
                         "anything about giving it up")
        state.clear_claim(chat)
        self.assertEqual(state.reap(set(), server="charter"), [chat])

    def test_giving_up_a_claim_that_was_never_made_is_a_no_op(self):
        """Never raises and never creates, like `clear_exit` beside it: this runs on the
        launch path, where a directory an older charter left — or one whose marker could
        not be written — must degrade rather than take the launch down."""
        state.bump("f-1")
        state.clear_claim("f-1")            # no marker to remove
        state.clear_claim("nothing.9")      # no directory at all
        # And a name `contain.child` refuses to shape into one — `$CHARTER_SESSION_ID` is
        # a value from the environment, so "there is no directory" and "there could not
        # be one" are two different answers and both have to be no-ops here.
        state.clear_claim("../../etc")
        self.assertFalse(state.frame_dir("nothing.9").exists(),
                         "giving up a claim created the directory it was asked about")
        with mock.patch("charter.frame.state.Path.unlink",
                        side_effect=OSError("read-only")):
            state.clear_claim("f-1")        # must not raise

    def test_a_marker_that_is_not_a_pid_keeps_nothing(self):
        """A truncated write, a hand-edited file, a `0` — `kill(2)`'s "every process in my
        group" rather than a process. Each is no claim about any process at all, and the
        safe direction is the one the module already takes for an unreadable record: the
        chat reaps exactly as it did before there was a marker."""
        for junk in ("0", "-1", "notapid", "", "12 34"):
            with self.subTest(junk=junk):
                chat = state.new_chat_id("api")
                (state.frame_dir(chat) / state._CLAIM_FILE).write_text(junk)
                self.assertEqual(state.reap(set(), server="charter"), [chat])


class TheIdIsANameAndNotAPointer(PersonaIso, unittest.TestCase):
    """Renaming a workspace under live chats changes nothing, because nothing ever parses
    an id for meaning. `frame_workspace` reads the workspace out of the frame's own file,
    which can be repointed."""

    def test_two_live_chats_survive_their_workspace_being_renamed(self):
        one, two = state.new_chat_id("oldname"), state.new_chat_id("oldname")
        for fid in (one, two):
            state.record_server(fid, "charter")
            state.record_workspace(fid, "oldname")
        self.assertEqual([one, two], ["oldname.1", "oldname.2"])

        for fid in (one, two):
            state.record_workspace(fid, "newname")

        # Both still resolve, both keep the old-spelled id, and the bar reads the file.
        for fid in (one, two):
            with self.subTest(fid=fid):
                self.assertTrue(fid.startswith("oldname."),
                                "an id was rewritten — every $CHARTER_SESSION_ID already "
                                "exported into a live process would now name nothing")
                self.assertEqual(state.frame_workspace(fid), "newname")
                self.assertTrue(state.frame_dir(fid).is_dir())
        self.assertEqual(state.reap({one, two}, server="charter"), [])

    def test_a_chat_allocated_after_the_rename_is_spelled_for_the_new_name(self):
        """The one visible cost, and it is deliberately not fixed: allocation scans for
        `{new name}.*`, so a renamed workspace's next chat is `newname.1` beside a
        sibling still called `oldname.2`. Ugly, harmless, and rewriting ids to tidy it
        would break every id already exported into a running process."""
        state.new_chat_id("oldname")
        state.new_chat_id("oldname")
        self.assertEqual(state.new_chat_id("newname"), "newname.1")


class Version(PersonaIso, unittest.TestCase):
    def test_a_fresh_frame_has_a_version(self):
        self.assertTrue(state.version("f-1"))

    def test_bumping_changes_it(self):
        before = state.version("f-1")
        state.bump("f-1")
        self.assertNotEqual(before, state.version("f-1"))

    def test_reading_an_unknown_frames_version_creates_nothing_on_disk(self):
        """A probe reads; it does not act (charter/news.py). `version()` on a frame that
        was never bumped must not create the directory it is only trying to look at —
        that is the mistake correction 1 exists to rule out."""
        self.assertEqual(state.version("never-bumped"), "0")
        self.assertFalse(state.frame_dir("never-bumped").exists())

    def test_a_non_utf8_version_file_degrades_to_the_sentinel_rather_than_raising(self):
        """Fix round 2, item 1: `read_text()` on bytes that are not valid UTF-8 raises
        `UnicodeDecodeError` — a `ValueError` subclass, never caught by an `except
        OSError` alone. `panel._tick` reads this function directly with nothing of its
        own guarding the call, so an uncaught decode error here used to reach a real
        panel's run loop and kill the pane — exactly the failure this module's own
        docstring already promised could not happen ("nothing here raises... a missing
        frame answers with the sentinel"). A corrupt file is treated the same as a
        missing one: the sentinel, not an exception."""
        d = state.frame_dir("f-1", create=True)
        (d / "version").write_bytes(b"\xff\xfe not valid utf-8 \x80\x81")
        self.assertEqual(state.version("f-1"), "0")

    def test_reap_then_version_does_not_resurrect_the_directory(self):
        """If `version()` ever called `bump()` on a miss, this would fight `reap()`
        forever: reap deletes, the next poll recreates, reap deletes again.

        The pid in the name is load-bearing since #383 — `reap` refuses to remove a
        directory whose launcher is still running, so the frame this test needs reaped
        has to be named after a pid that is genuinely over. `gone-1` used to read as a
        throwaway label; pid 1 is `launchd`/`init`, and reap would now (rightly) keep it.
        """
        gone = f"gone-{_a_dead_pid()}"
        state.bump(gone)
        state.reap(set(), server="charter")
        self.assertEqual(state.version(gone), "0")
        self.assertFalse(state.frame_dir(gone).exists())


class FrameDirContainment(PersonaIso, unittest.TestCase):
    def test_a_hostile_fid_cannot_resolve_outside_the_frame_root(self):
        """`fid` is not always minted by this module — a later caller reads it out of
        $CHARTER_SESSION_ID, so a traversal, an absolute path, or an embedded separator
        must be refused rather than silently rewritten into a safe-looking name."""
        root = state._root()
        for hostile in ("../../etc/passwd", "/etc/passwd", "a/b", "..", "."):
            with self.subTest(hostile=hostile):
                self.assertIsNone(state.frame_dir(hostile))
                self.assertIsNone(state.frame_dir(hostile, create=True))

    def test_no_directory_is_created_for_a_hostile_fid(self):
        state.frame_dir("../../escaped", create=True)
        self.assertFalse(state._root().exists())

    def test_bump_on_a_hostile_fid_does_not_raise(self):
        """bump() runs from charter's hooks, where an exception costs a session its
        turn — a malformed $CHARTER_SESSION_ID must be a no-op, not a crash."""
        state.bump("../../escaped")  # must not raise
        self.assertFalse(state._root().exists())


class OverlongFid(PersonaIso, unittest.TestCase):
    """`contain.child` bounds shape, not length — a 5000-character fid passes it and
    then hits `mkdir`'s own ENAMETOOLONG, which is reachable from a real
    `$CHARTER_SESSION_ID` (`session.py`'s id-safety regex strips characters, never
    bounds length). `bump`/`record_exit` run from hooks, where that has to degrade to a
    no-op rather than propagate."""

    def test_bump_on_an_overlong_fid_does_not_raise_or_create(self):
        fid = "x" * 5000
        state.bump(fid)  # must not raise
        self.assertFalse(state._root().exists())
        self.assertEqual(state.version(fid), "0")

    def test_record_exit_on_an_overlong_fid_does_not_raise_or_create(self):
        fid = "x" * 5000
        state.record_exit(fid, 7)  # must not raise
        self.assertFalse(state._root().exists())
        self.assertIsNone(state.exit_code(fid))


class WriteFailureIsNotFatal(PersonaIso, unittest.TestCase):
    """The over-long-fid case above fails at `mkdir`, before any write is attempted.
    This covers the other half: the directory exists, but the write into it fails
    anyway (a filesystem that fills up between `mkdir` and `os.replace`, say) — still a
    hook's-eye no-op, not a raise."""

    def test_bump_survives_a_failing_replace(self):
        with mock.patch("charter.frame.state.os.replace", side_effect=OSError("disk full")):
            state.bump("f-1")  # must not raise
        self.assertEqual(state.version("f-1"), "0")

    def test_record_exit_survives_a_failing_replace(self):
        with mock.patch("charter.frame.state.os.replace", side_effect=OSError("disk full")):
            state.record_exit("f-1", 9)  # must not raise
        self.assertIsNone(state.exit_code("f-1"))

    def test_a_failed_bump_leaves_the_previous_version_intact(self):
        """This is the property the tmp-file + os.replace shape in `bump()` exists for,
        and Finding 1 made it load-bearing: a failed write no longer raises, so the only
        thing standing between it and a reader seeing a corrupted value is that
        os.replace never touches the target file unless it fully succeeds. A bump that
        fails must leave the version a reader already saw exactly as it was — not "0",
        not empty, not some partial write of the new value."""
        state.bump("f-1")
        before = state.version("f-1")
        with mock.patch("charter.frame.state.os.replace", side_effect=OSError("disk full")):
            state.bump("f-1")  # the write fails silently (Finding 1)
        self.assertEqual(state.version("f-1"), before)

    def test_a_claim_whose_marker_cannot_be_written_is_still_a_claim(self):
        """#685's marker is written on the launch path, so it takes the same posture as
        every other writer here: a filesystem that will not take the file costs the claim
        its keep-rule, never the launch. The ordinal still comes back and the directory is
        still this launcher's."""
        with mock.patch("charter.frame.state.config.write_for",
                        side_effect=OSError("disk full")):
            chat = state.new_chat_id("api")   # must not raise
        self.assertEqual(chat, "api.1")
        self.assertTrue(state.frame_dir(chat).is_dir())


class ExitCode(PersonaIso, unittest.TestCase):
    def test_an_unfinished_frame_has_no_exit_code(self):
        self.assertIsNone(state.exit_code("f-1"))

    def test_the_recorded_code_comes_back(self):
        state.record_exit("f-1", 42)
        self.assertEqual(state.exit_code("f-1"), 42)


class ClearExit(PersonaIso, unittest.TestCase):
    """A new frame adopting a recycled pid inherits the directory of the frame that had
    that pid before it (#383 keeps a directory while its pid is live, and on a launch
    that pid is the launcher's own). `clear_exit` is what stops it inheriting the dead
    frame's exit code along with the directory."""

    def test_a_recorded_code_is_gone_afterwards(self):
        state.record_exit("f-1", 99)
        state.clear_exit("f-1")
        self.assertIsNone(state.exit_code("f-1"))

    def test_the_version_a_panel_polls_is_left_alone(self):
        """Only `exit` is stale on a relaunch. `version` is a monotonic counter panels
        compare against their last reading, and moving it backwards — or removing it —
        is `bump`'s business, not this function's."""
        state.bump("f-1")
        before = state.version("f-1")
        state.clear_exit("f-1")
        self.assertEqual(state.version("f-1"), before)

    def test_clearing_a_frame_that_was_never_recorded_creates_nothing(self):
        """It runs on the launch path against an id that usually has no directory at
        all — the ordinary first launch for a workspace. A read must not mint one, the
        same rule `version()` follows."""
        state.clear_exit("never-existed")
        self.assertFalse(state.frame_dir("never-existed").exists())

    def test_clearing_a_hostile_fid_does_not_raise(self):
        state.clear_exit("../../escaped")  # must not raise
        self.assertFalse(state._root().exists())

    def test_clearing_survives_a_failing_unlink(self):
        """Nothing in this module raises: a launch is not worth failing over a file
        that could not be deleted."""
        state.record_exit("f-1", 99)
        with mock.patch("charter.frame.state.Path.unlink", side_effect=OSError("read-only")):
            state.clear_exit("f-1")  # must not raise


class RespawnAttempts(PersonaIso, unittest.TestCase):
    """The counter that stops a broken panel respawning forever.

    A panel pane's `pane-died` hook survives the respawn it triggers (verified against
    real tmux 3.7c: `show-hooks -p` reads the hook back unchanged after `respawn-pane`),
    so a panel that dies instantly on every start would respawn in a hot loop with
    nothing anywhere counting. tmux cannot count; this is where the count lives.
    """

    def setUp(self) -> None:
        super().setUp()
        # `respawn_attempt` never creates a frame's directory (see its own docstring),
        # so these tests stand up the state a live frame always already has: `cmd_launch`
        # creates and bumps it before a single pane is split.
        for fid in ("f-1", "f-2"):
            state.bump(fid)

    def test_the_first_attempt_is_one_and_each_call_claims_the_next(self):
        self.assertEqual(state.respawn_attempt("f-1", "top"), 1)
        self.assertEqual(state.respawn_attempt("f-1", "top"), 2)
        self.assertEqual(state.respawn_attempt("f-1", "top"), 3)

    def test_each_slot_counts_on_its_own(self):
        """One broken panel must not spend another's attempts — a `left` renderer that
        crashes on every start would otherwise use up `bottom`'s budget and stop a
        perfectly healthy panel being brought back after an unrelated death."""
        state.respawn_attempt("f-1", "top")
        state.respawn_attempt("f-1", "top")
        self.assertEqual(state.respawn_attempt("f-1", "bottom"), 1)

    def test_each_frame_counts_on_its_own(self):
        state.respawn_attempt("f-1", "top")
        self.assertEqual(state.respawn_attempt("f-2", "top"), 1)

    def test_a_frame_already_reaped_cannot_count_and_is_not_recreated(self):
        """Counting must not resurrect a directory `reap` has deleted — the hazard this
        module's own docstring records for `version()`, reached here through a write
        path instead of a read.

        Reached from a real frame's teardown, though less often since #383: the panels
        all die when the session is killed, so every panel's `pane-died` hook fires on
        the way out, and `reap` now KEEPS a directory whose launcher pid is still live —
        which the launcher's own closing `reap()` always is. So the directory usually
        survives that moment and is taken by a later launch instead; this is the case
        after that, where the count arrives at a name nothing owns any more. `f-gone`
        carries no pid at all (`_launcher_pid` needs `<name>-<digits>`), so `reap`
        removes it exactly as it did before #383."""
        state.bump("f-gone")
        state.reap(set(), server="charter")
        self.assertIsNone(state.respawn_attempt("f-gone", "top"))
        self.assertFalse(state.frame_dir("f-gone").exists(),
                         "counting a respawn recreated a frame directory reap removed")

    def test_a_frame_id_the_directory_layer_refuses_cannot_count(self):
        """`None`, never a number — and the caller reads that as "give up", not as
        "attempt zero". A count that cannot be recorded is exactly the state in which
        respawning is unbounded, so the safe degrade is to stop, leaving the dead pane
        and its own message visible."""
        self.assertIsNone(state.respawn_attempt("../../etc", "top"))

    def test_a_slot_name_with_a_separator_is_refused_rather_than_joined(self):
        """The slot is part of a FILE name. `commands_frame.cmd_respawn` already refuses
        a slot with no renderer before reaching here, but this module's own rule is that
        a name handed to it is resolved through `contain.child` rather than trusted by
        whoever called it (see `frame_dir`'s own docstring).

        `../y`, not the obvious `../../../etc/passwd`, and the difference IS the test.
        The obvious one lands under directories that do not exist, so a version with no
        containment check at all still answers `None` — from the failed write, not from
        any refusal — and the test passes green over a deleted guard (confirmed by
        mutation twice: the first shape of both this test and the code under it was
        green with `contain.child` replaced by a bare join). `../y` climbs exactly one
        level, out of the per-slot directory and back into the frame's own, where the
        write really would succeed — so only a refusal can produce `None`, and the file
        it would have left behind is checked for directly.
        """
        d = state.frame_dir("f-1", create=True)
        self.assertIsNone(state.respawn_attempt("f-1", "../y"))
        self.assertFalse((d / "y").exists(),
                         "the slot name was joined onto the path instead of refused")
        self.assertIsNone(state.respawn_attempt("f-1", "../../../etc/passwd"))

    def test_a_write_that_fails_cannot_count_rather_than_raising(self):
        """Same must-not-raise promise as `bump`: this runs from a tmux hook.

        **Not `Path.write_text` any more, and the reason is the change that broke it.**
        This used to patch `pathlib.Path.write_text`, which pinned the test to the WRITER'S
        SPELLING rather than to the property it is named for: routing the writer through
        `config.write_for` (#582) left the mock aimed at a call nobody makes, the write
        succeeded, and the case went red having found nothing. `config.write_for` is what
        `frame/state.py` actually depends on, so that is what a failing filesystem is
        injected at — the same shape `test_frame_gather` uses for its unlink.
        """
        state.respawn_attempt("f-1", "top")
        with mock.patch.object(state.config, "write_for", side_effect=OSError("full")):
            self.assertIsNone(state.respawn_attempt("f-1", "top"))


class ClearRespawn(PersonaIso, unittest.TestCase):
    """The other half of `clear_exit`'s bill, for the counter rather than the exit code.

    Same cause: since #383 `reap` keeps a directory while the pid in its name is live,
    so a launcher landing on a recycled pid for the SAME workspace mints the same id and
    adopts the previous frame's whole directory. `respawn_attempt` never resets, so an
    adopted count is a budget already spent on deaths that happened to another frame.
    """

    def test_the_next_attempt_starts_from_one_again(self):
        state.bump("f-1")
        state.respawn_attempt("f-1", "top")
        state.respawn_attempt("f-1", "top")
        state.clear_respawn("f-1")
        self.assertEqual(state.respawn_attempt("f-1", "top"), 1)

    def test_every_slot_is_cleared_not_only_the_one_that_died(self):
        """A frame's panels each keep their own file, and all of them are the previous
        frame's. Clearing one slot would leave the rest of the new frame's panels with a
        budget spent by a frame they were never part of."""
        state.bump("f-1")
        for slot in ("top", "bottom", "left"):
            state.respawn_attempt("f-1", slot)
        state.clear_respawn("f-1")
        for slot in ("top", "bottom", "left"):
            self.assertEqual(state.respawn_attempt("f-1", slot), 1, slot)

    def test_only_this_frames_counts_go(self):
        state.bump("f-1")
        state.bump("f-2")
        state.respawn_attempt("f-2", "top")
        state.clear_respawn("f-1")
        self.assertEqual(state.respawn_attempt("f-2", "top"), 2)

    def test_the_version_a_panel_polls_is_left_alone(self):
        """Same rule `clear_exit` follows: moving the counter panels compare against is
        `bump`'s business, and `cmd_launch` calls it one line later anyway."""
        state.bump("f-1")
        before = state.version("f-1")
        state.clear_respawn("f-1")
        self.assertEqual(state.version("f-1"), before)

    def test_clearing_a_frame_that_never_counted_creates_nothing(self):
        """The ordinary first launch for a workspace has no directory here at all, and
        a launch must not mint one just to empty it."""
        state.clear_respawn("never-existed")
        self.assertFalse(state.frame_dir("never-existed").exists())

    def test_clearing_a_hostile_fid_does_not_raise(self):
        state.clear_respawn("../../escaped")  # must not raise
        self.assertFalse(state._root().exists())


class Reap(PersonaIso, unittest.TestCase):
    """Every fixture in here is named after a pid that has genuinely exited, the KEPT
    ones as deliberately as the removed ones.

    Up to #383 the trailing number in these names was decoration (`dead-1`, `live-1`,
    `old-1`); `reap` now reads it as the launcher's pid, and **pid 1 is `launchd`/`init`,
    which never exits**. On the remove side that is loud — a fixture reap refuses to
    remove fails its own assertion. On the KEEP side it is silent and worse: `live-1`
    and `old-1` were kept by the pid rule, the `live` argument stopped deciding anything,
    and both tests passed with the live-session check deleted outright. `_a_dead_pid()`
    on both sides puts membership in `live` back to being the only thing that can keep
    a directory here.
    """

    def test_a_directory_whose_session_is_gone_is_removed(self):
        dead = f"dead-{_a_dead_pid()}"
        live = f"live-{_a_dead_pid()}"
        state.bump(dead)
        state.bump(live)
        removed = state.reap({live}, server="charter")
        self.assertEqual(removed, [dead])
        self.assertFalse(state.frame_dir(dead).exists())
        self.assertTrue(state.frame_dir(live).exists(),
                        "the live session's directory was reaped — and since its "
                        "launcher is dead, `live` is the only thing that could have "
                        "saved it")

    def test_a_live_frame_is_never_reaped_by_age(self):
        """A long-lived frame is exactly what an age heuristic would eat.

        The pid is a dead one on purpose (see the class docstring): this is the test
        that pins "reap never deletes by age", so the only reason its fixture may
        survive is the session being live."""
        old = f"old-{_a_dead_pid()}"
        state.bump(old)
        self.assertEqual(state.reap({old}, server="charter"), [])

    def test_a_sibling_exit_code_survives_a_reap_that_beats_its_own_launcher(self):
        """#383. `reap` runs at EVERY frame launch, and the set it is handed names the
        tmux sessions live at that instant. A sibling frame whose session has just ended
        is therefore absent from it while its own launcher is still inside `cmd_launch`,
        one line short of reading the `exit` file it just recorded. Removing the
        directory there does not merely lose bookkeeping: `exit_code` answers `None`,
        `cmd_launch` turns that into a returned 0, and a harness that actually failed is
        reported as a success to whatever `&&` chain or CI step called charter.

        This test process's own pid stands in for that launcher. It is not a charter
        launcher, which is the point — `reap` cannot tell the difference and must not
        try: all it may ask is whether the process named at the end of the directory is
        still there to come back for its answer."""
        fid = state.frame_id("sibling", os.getpid())
        state.record_exit(fid, 42)
        state.reap({"some-other-frames-session"}, server="charter")
        self.assertEqual(state.exit_code(fid), 42,
                         "reap deleted a live launcher's frame directory, and with it "
                         "the exit code that launcher had not read yet")

    def test_a_frame_whose_launcher_has_exited_is_still_removed(self):
        """The other half of #383, and the one that keeps the fix from being a no-op
        dressed as a fix: once the pid in the name is gone there is nobody left to read
        the `exit` file, so the directory is `reap`'s to remove exactly as before."""
        fid = state.frame_id("finished", _a_dead_pid())
        state.bump(fid)
        self.assertEqual(state.reap(set(), server="charter"), [fid])
        self.assertFalse(state.frame_dir(fid).exists())

    def test_a_directory_that_names_no_pid_is_still_removed(self):
        """Not everything under the frame root was minted by `frame_id`: debris, a
        hand-made directory, a name from an older charter. With no pid to ask about,
        the live-session test is the only evidence there is — and it is the one `reap`
        already had, so an unparseable name must not become undeletable."""
        state.bump("debris")
        self.assertEqual(state.reap(set(), server="charter"), ["debris"])

    def test_a_bare_number_is_not_read_as_a_pid(self):
        """`frame_id` always emits `<workspace>-<pid>` with a non-empty workspace (its
        `or "frame"` fallback guarantees one), so a directory that is nothing but digits
        did not come from it and those digits are not a claim about any process. Named
        after THIS process's pid — as live as a pid gets — so reading it as one would
        make the directory undeletable for as long as the suite runs."""
        name = str(os.getpid())
        state.bump(name)
        self.assertEqual(state.reap(set(), server="charter"), [name])

    def test_a_trailing_zero_is_not_read_as_a_pid(self):
        """`kill(2)` reads 0 as "every process in my group", not as a process, so
        `os.kill(0, 0)` SUCCEEDS and a frame named `ws-0` would look alive forever.
        `frame_id` can only ever have written a real `os.getpid()` there, and that is
        never 0 — so the number is debris and the directory stays reapable."""
        state.bump("ws-0")
        self.assertEqual(state.reap(set(), server="charter"), ["ws-0"])

    def test_a_launcher_this_user_may_not_signal_still_counts_as_alive(self):
        """EPERM is an ANSWER, and the opposite of what it looks like. `os.kill(pid, 0)`
        raises `PermissionError` for a process that exists and belongs to somebody else —
        another operator's frame on a shared machine, or one launched under `sudo`.
        Read as "gone", it would make every such frame reapable while its harness was
        still running, which is #383 again with a different cast.

        Forced rather than found: pid 1 answers this way for an unprivileged run and
        answers plain success for a root CI container, so asking the real machine would
        pin the branch on a laptop and quietly stop pinning it in CI."""
        state.bump("another-users-frame-4242")
        with mock.patch("charter.frame.state.os.kill",
                        side_effect=PermissionError(1, "Operation not permitted")):
            self.assertEqual(state.reap(set(), server="charter"), [])

    def test_liveness_is_never_asked_off_posix(self):
        """`os.kill(pid, 0)` is a question on POSIX and an ANSWER on Windows, where it
        maps to `TerminateProcess` — asking it there would kill whatever process the
        number in a directory name happened to land on. `news._outer_probe` documents
        the same trap and this file's own suite pins it there; this pins it here, where
        the number comes off a filesystem name rather than an environment variable.

        Asserted on the helper rather than through `reap`, and not for tidiness:
        `mock.patch` of `os.name` is global for its duration, and `_root()` builds a
        `Path` — under a patched `os.name` pathlib refuses to instantiate at all
        ("cannot instantiate 'WindowsPath' on your system"), so a test driving the whole
        of `reap` would fail on the wrong line and prove nothing about `os.kill`. The
        pid handed over is real and live (ours), so a helper that asked anyway would get
        a truthful "alive" back — the assertion has to be that the question was never
        PUT, not that the answer came out a particular way."""
        with mock.patch("charter.frame.state.os.name", "nt"), \
             mock.patch("charter.frame.state.os.kill") as kill:
            self.assertTrue(state._launcher_is_alive(os.getpid()))
        kill.assert_not_called()

    def test_a_number_too_large_to_be_a_pid_neither_raises_nor_survives(self):
        """`reap` runs on the launch path, where this module's own docstring promises
        nothing raises. A trailing number beyond what a `pid_t` can hold parses as an
        int perfectly well and then makes `os.kill` raise `OverflowError` — which is
        NOT an `OSError`, so guarding only that would let it escape into a launch. It
        also names no process, so the directory stays reapable."""
        name = "ws-99999999999999999999"
        state.bump(name)
        self.assertEqual(state.reap(set(), server="charter"), [name])


class ReapAcrossServers(PersonaIso, unittest.TestCase):
    """A frame lives on ONE tmux server, and only that server can say it is dead.

    Charter now runs frames on two: its own private one (`tmux -L charter`, sessions
    named by frame id) and, when charter is started from inside a tmux the operator
    already has, THEIRS (`tmux -S <socket>`, windows named by frame id). Neither
    server's liveness list mentions the other's frames at all, so an unscoped `reap`
    deletes the other's state on sight — a running frame's panels lose the version file
    they poll, and its recorded exit code goes with it, while the frame itself is still
    on screen. The frame's own server is written down when its directory is created and
    checked here.

    Every fixture here is named after a pid that has genuinely exited, for the reason
    `Reap`'s own docstring gives: since #383 `reap` reads the trailing number as the
    launcher's pid and keeps any directory whose launcher is still running. `mine-1` and
    `theirs-1` read as throwaway labels, but pid 1 is `launchd`/`init` — the pid rule
    would have kept every one of them and these tests would have passed with the server
    check deleted outright.
    """

    THEIRS = OPERATOR_SOCKET

    def _frame_on(self, stem, server):
        fid = f"{stem}-{_a_dead_pid()}"
        state.bump(fid)
        state.record_server(fid, server)
        return fid

    def test_a_frame_on_another_server_survives_this_servers_reap(self):
        mine = self._frame_on("mine", "charter")
        theirs = self._frame_on("theirs", self.THEIRS)
        self.assertEqual(state.reap(set(), server="charter"), [mine])
        self.assertTrue(state.frame_dir(theirs).exists(),
                        "the other server's frame was reaped — and since its launcher "
                        "is dead too, the recorded server is the only thing that could "
                        "have saved it")

    def test_the_other_server_reaps_its_own(self):
        """The same test from the other side, so a `reap` that simply never removed
        anything would not pass both."""
        mine = self._frame_on("mine", "charter")
        theirs = self._frame_on("theirs", self.THEIRS)
        self.assertEqual(state.reap(set(), server=self.THEIRS), [theirs])
        self.assertTrue(state.frame_dir(mine).exists())

    def test_a_live_frame_on_this_server_is_still_kept(self):
        gone = self._frame_on("mine", "charter")
        live = self._frame_on("mine", "charter")
        self.assertEqual(state.reap({live}, server="charter"), [gone])

    def test_a_live_launcher_on_this_server_is_still_kept(self):
        """The two guards are independent and BOTH are asked (#381 + #383). This one
        matches the server exactly and is absent from `live`, so the server check has
        nothing left to say — only the pid rule can keep it, and it must, or #383's
        fix stops reaching frames that record a server (which, since #381, is all of
        them)."""
        fid = state.frame_id("sibling", os.getpid())
        state.record_server(fid, "charter")
        state.record_exit(fid, 42)
        self.assertEqual(state.reap(set(), server="charter"), [])
        self.assertEqual(state.exit_code(fid), 42)

    def test_a_frame_from_before_charter_recorded_this_is_still_reapable(self):
        """The migration case, and the one place an unknown server matches every
        server. A directory with no marker was written by a charter that only ever ran
        frames on its own private server; leaving it unreapable forever would trade a
        transient bug for a permanent leak."""
        fid = f"legacy-{_a_dead_pid()}"
        state.bump(fid)
        self.assertIsNone(state.frame_server(fid))
        self.assertEqual(state.reap(set(), server=self.THEIRS), [fid])

    def test_the_recorded_server_reads_back(self):
        state.record_server("f-1", self.THEIRS)
        self.assertEqual(state.frame_server("f-1"), self.THEIRS)

    def test_recording_a_server_for_an_id_no_directory_can_be_made_for_is_a_no_op(self):
        """`record_server` runs on the launch path, where an id `contain.child` refuses
        must degrade rather than raise — the same promise every other writer in this
        module makes."""
        state.record_server("../escape", "charter")
        self.assertIsNone(state.frame_server("../escape"))


class TheFramesOwnWorkspace(PersonaIso, unittest.TestCase):
    """`record_workspace`/`frame_workspace` — #512.

    A frame is launched FOR a workspace, and no process inside it can work out which one:
    `workspace.resolve`'s deciding rungs are a `$CHARTER_WORKSPACE` the launcher usually
    does not have, a cwd that is the plane root, a per-session pointer keyed on an id that
    inside a frame names the FRAME, and a per-terminal pointer keyed on the asking pane.
    The launcher is one ordinary shell in the operator's own terminal and answers all
    three; a panel answers none of them, and falls to `default`. So the launcher writes
    the answer down.
    """

    def test_the_recorded_workspace_reads_back(self):
        state.record_workspace("f-1", "harness-wrapper")
        self.assertEqual(state.frame_workspace("f-1"), "harness-wrapper")

    def test_a_frame_nobody_recorded_one_for_says_it_does_not_know(self):
        """`None`, never a guessed name. The migration case (a frame launched by a
        charter that predates this and still running across the upgrade) and the failed
        write are the same fact — "do not take this frame's workspace from here" — and
        `slots._frame_workspace` is what decides what to do instead."""
        state.bump("f-never-recorded")
        self.assertIsNone(state.frame_workspace("f-never-recorded"))

    def test_a_relaunch_on_the_same_id_overwrites_rather_than_keeps(self):
        """The recycled-pid case #383 is about, on this file. A frame id is
        `<workspace>-<launcher pid>` and `reap` keeps a directory while that pid is live —
        which on a launch it is, because it is the launcher's own. An adopted `workspace`
        is another frame's answer, so every launch rewrites it, exactly as `record_server`
        does with its own marker."""
        state.record_workspace("f-1", "an-older-frames-workspace")
        state.record_workspace("f-1", "this-frames-workspace")
        self.assertEqual(state.frame_workspace("f-1"), "this-frames-workspace")

    def test_a_name_that_could_escape_the_workspaces_directory_is_refused_on_read(self):
        """The value is joined onto `workspaces/` by `workspace_dir()` and drawn on a
        panel's screen. #442 is what an unchecked `../../` in that position already cost
        once, through `workspace.declared_default`; this keeps the same rule
        (`workspace.valid_name`) on charter's own copy of the same kind of value.

        Written past `record_workspace` deliberately — the writer is charter's own
        launcher and never produces this, so a test that went through it would be pinning
        the writer rather than the reader that has to survive a corrupt file."""
        d = state.frame_dir("f-1", create=True)
        (d / "workspace").write_text("../../escaped\n")
        self.assertIsNone(state.frame_workspace("f-1"))

    def test_an_empty_recorded_workspace_is_not_known_either(self):
        """A truncated write is the shape that would otherwise pass the truthiness test
        one layer up and hand `workspace_dir()` the `workspaces/` directory itself."""
        d = state.frame_dir("f-1", create=True)
        (d / "workspace").write_text("\n")
        self.assertIsNone(state.frame_workspace("f-1"))

    def test_recording_for_an_id_no_directory_can_be_made_for_is_a_no_op(self):
        """The launch path's own promise, kept here too: an id `contain.child` refuses
        degrades to "charter does not know" rather than taking the launch down."""
        state.record_workspace("../escape", "demo")
        self.assertIsNone(state.frame_workspace("../escape"))

    def test_reading_never_creates_the_directory_it_looked_in(self):
        """The rule the whole module keeps and `version`'s docstring states: a read must
        not resurrect a directory `reap()` has just removed."""
        self.assertIsNone(state.frame_workspace("f-never-existed"))
        self.assertFalse(state.frame_dir("f-never-existed").exists(),
                         "a read minted the frame directory it was only looking in")


class ThePinOutranksTheFramesOwnRungs(PersonaIso, unittest.TestCase):
    """`workspace_for`'s rung 0. `$CHARTER_WORKSPACE` beats every rung below it, because
    that is what the variable means everywhere else charter reads it.

    Nothing in the frame is allowed to draw a workspace the session's own commands will
    not act on, and the pin is the one rung that decides for BOTH: `workspace.resolve`
    puts it above every pointer, so a `charter ws` command run at the agent answers the
    pinned name no matter what the frame recorded or what pointer the session wrote. A
    panel that ranked the per-session pointer first drew `other` while every command in
    that session worked in the pinned workspace.

    That is also the mechanism charter itself tells operators to use for parallel and
    unattended agents — `hooks`' own nudge says to "re-launch with `CHARTER_WORKSPACE`
    set" — so it is precisely the case where nobody is watching the panel closely enough
    to catch it lying.
    """

    def setUp(self) -> None:
        super().setUp()
        # Cleared rather than merely overwritten: `resolve`'s rungs below the pin read
        # `$CHARTER_SESSION_ID` and `$TMUX_PANE` out of whatever terminal the suite is
        # being run from, so a developer inside a live frame would otherwise be supplying
        # half this fixture (#519, #521).
        self.enterContext(mock.patch.dict(os.environ, {}, clear=True))

    def _pin(self, name: str) -> None:
        os.environ["CHARTER_WORKSPACE"] = name

    def test_a_workspace_chosen_inside_the_frame_reaches_neither_it_nor_its_commands(self):
        """The regression, and since #791 it is a statement about the pointer rather than
        about the pin's rank over it.

        A pin is set, `charter workspace use other` is typed inside the frame, and neither
        the frame's own answer nor the session's own commands move: the pointer is not a
        rung of `state.own_workspace` at all any more, and `workspace.resolve` ranks
        `$CHARTER_WORKSPACE` above it — which is what `commands_workspace` warns about in as
        many words when it says `ws use` will not stick while the variable is set.

        **The launch record names a third workspace deliberately.** Without that the pin
        rung is unmeasurable here: `frame_workspace` would answer `zeta` too, and this case
        would pass whether or not anything read the pin. Was
        `test_the_pin_beats_a_workspace_chosen_inside_the_frame`, whose fixture leaned on
        the pointer being the disagreeing rung."""
        from charter import workspace as ws
        self._pin("zeta")
        state.record_workspace("f-1", "recorded-at-launch")
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "f-1"}):
            ws.set_active("other", force=True)
        self.assertEqual(ws.for_session("f-1"), "other",
                         "the fixture never wrote the pointer this test is about")
        self.assertEqual(state.workspace_for("f-1"), "zeta")
        self.assertEqual(ws.resolve(), "zeta",
                         "the frame and the session's own commands must not disagree")

    def test_the_pin_beats_what_the_launcher_recorded(self):
        """The second rung, checked separately so the test above cannot pass merely
        because the record happened to be missing."""
        self._pin("zeta")
        state.record_workspace("f-1", "recorded-at-launch")
        self.assertEqual(state.workspace_for("f-1"), "zeta")

    def test_with_no_pin_the_frames_own_rungs_decide(self):
        """The ordinary case, and the guard against a rung 0 that answers when it should
        not: an unset variable must leave the chain exactly as it was."""
        state.record_workspace("f-1", "recorded-at-launch")
        self.assertEqual(state.workspace_for("f-1"), "recorded-at-launch")

    def test_a_pin_that_cannot_name_a_workspace_is_not_drawn(self):
        """The value reaches `workspace_dir()`'s join and a panel's screen, so it is
        name-checked here on the same terms as `frame_workspace` — #442 is what an
        unchecked `../../` in that position cost once already. A pin charter cannot use
        falls through rather than being rendered."""
        self._pin("../../escaped")
        state.record_workspace("f-1", "recorded-at-launch")
        self.assertEqual(state.workspace_for("f-1"), "recorded-at-launch")

    def test_the_pin_is_stripped_the_way_resolve_strips_it(self):
        """`workspace.resolve` returns `env.strip()`, so a variable exported with padding
        (a here-doc, a `.env` line, an `export CHARTER_WORKSPACE=$(…)` with a trailing
        newline) names a real workspace to every command the session runs. A frame that
        compared the raw value would find no such name, fall through, and draw something
        else — the disagreement this whole rung exists to close, arriving through a space."""
        from charter import workspace as ws
        self._pin("  zeta\n")
        state.record_workspace("f-1", "recorded-at-launch")
        self.assertEqual(ws.resolve(), "zeta",
                         "the fixture no longer matches what `resolve` does with padding")
        self.assertEqual(state.workspace_for("f-1"), "zeta")

    def test_an_empty_pin_is_no_pin(self):
        """`CHARTER_WORKSPACE=` exported empty is how a shell clears one, and
        `workspace.resolve` already treats it as absent. The name check answers this on
        its own terms — `valid_name("")` is False — so there is no separate truthiness
        guard here for a mutation to pass through."""
        self._pin("   ")
        state.record_workspace("f-1", "recorded-at-launch")
        self.assertEqual(state.workspace_for("f-1"), "recorded-at-launch")


class ANotice(PersonaIso, unittest.TestCase):
    """`state.say` / `state.notice` / `state.notice_expiry` — the surface a switch
    outcome moved onto with #729.

    What it replaced was `display-message -d 4000`, and the two measurements that moved
    it are recorded on `state.say` itself: a tmux client suspends its PANE redraw for the
    whole of a message's duration (4.03s of frozen screen on tmux 3.7c and at the 3.2
    floor alike), and `display-message -t <pane>` selects the format target rather than
    the client, so an outcome about one frame was drawn on whichever client attached most
    recently. Neither is reachable from a file a frame's own panel reads.
    """

    def test_a_notice_is_read_back(self):
        state.say("f-1", "charter: workspace \u2192 gamma")
        self.assertEqual(state.notice("f-1"), "charter: workspace \u2192 gamma")

    def test_a_frame_with_no_notice_has_nothing_to_say(self):
        self.assertEqual(state.notice("f-1"), "")

    def test_reading_a_notice_creates_nothing_on_disk(self):
        """`version`'s rule, for the same reason and the same caller: a panel polls this
        five times a second, and a read that created a directory would fight `reap`."""
        self.assertEqual(state.notice("never-noticed"), "")
        self.assertFalse(state.frame_dir("never-noticed").exists())

    def test_a_notice_stops_being_read_back_once_it_expires(self):
        """The property the whole dwell rests on. Written with a duration already spent,
        so this pins the EXPIRY rather than sleeping through one."""
        state.say("f-1", "charter: gone by now", seconds=-1)
        self.assertEqual(state.notice("f-1"), "")

    def test_the_expiry_is_when_it_stops_being_drawn(self):
        """`panel._watch` needs the deadline and not the text: the falling edge it has to
        repaint on is a clock crossing this number, and nothing else announces it."""
        before = time.time()
        state.say("f-1", "charter: still here", seconds=30)
        self.assertGreaterEqual(state.notice_expiry("f-1"), before + 30)

    def test_an_absent_notices_expiry_is_already_past(self):
        """`0.0` rather than `None`, so `time.time() < expiry` is the whole of the
        caller's question and there is no branch for "no notice" to get wrong."""
        self.assertEqual(state.notice_expiry("f-1"), 0.0)

    def test_a_newline_cannot_write_a_second_line_of_the_file(self):
        """The notice is stored as an expiry line and then the text, so a newline in the
        message is a value crossing into a format with structure — `contain.one_line`'s
        own case. Without it the text after the newline is silently dropped on read, and
        with a leading newline the message would vanish entirely."""
        state.say("f-1", "charter: first\nsecond")
        said = state.notice("f-1")
        self.assertNotIn("\n", said)
        self.assertIn("second", said)

    def test_a_corrupt_expiry_degrades_to_silence_rather_than_raising(self):
        """`slots._bottom` draws the one row `docs/frame.md` promises is never dropped,
        so a notice file it cannot parse must cost the line and never the row."""
        d = state.frame_dir("f-1", create=True)
        (d / "notice").write_text("not-a-number\ncharter: hello\n")
        self.assertEqual(state.notice("f-1"), "")
        self.assertEqual(state.notice_expiry("f-1"), 0.0)

    def test_a_non_utf8_notice_degrades_to_silence(self):
        d = state.frame_dir("f-1", create=True)
        (d / "notice").write_bytes(b"\xff\xfe\x80")
        self.assertEqual(state.notice("f-1"), "")
        self.assertEqual(state.notice_expiry("f-1"), 0.0)

    def test_a_hostile_fid_writes_nothing_and_says_nothing(self):
        """`frame_dir`'s containment, reached through the new writer. `say` takes an id
        that came off `$CHARTER_SESSION_ID` exactly as `bump` does."""
        state.say("../escape", "charter: nope")
        self.assertEqual(state.notice("../escape"), "")

    def test_a_refusal_is_given_longer_than_an_outcome_that_happened(self):
        """The one thing `ok` decides. A refusal is the outcome with no other surface —
        nothing moved, so no panel repaints into the answer — and it carries the fix in
        its own text."""
        self.assertGreater(state.REFUSAL_SECONDS, state.NOTICE_SECONDS)

    def test_an_empty_message_writes_no_notice(self):
        """Otherwise a blank line would take the row's top priority away from an alert
        for the whole dwell, saying nothing while it did — and keep `panel._watch`
        repainting five times a second to keep saying it.

        Asserted on the FILE, not on `notice()`: the reader strips too, so a version that
        wrote the blank notice and hid it on the way out would satisfy a `notice() == ""`
        check while still holding the dwell open for `notice_expiry`."""
        state.say("f-1", "   ")
        self.assertEqual(state.notice_expiry("f-1"), 0.0)
        self.assertFalse((state.frame_dir("f-1") / "notice").exists())

    def test_what_is_written_is_already_trimmed(self):
        """Asserted on the FILE, and it has to be: `notice()` strips on the way out too,
        so a writer that only `lstrip`ped would be invisible through the reader — which is
        exactly how the sweep found this line unpinned. The property is that what charter
        stores is already clean, because the row joins fields with ` · ` and a trailing
        space would draw as `charter: gamma  · 5 todos`."""
        state.say("f-1", "charter: gamma   ")
        raw = (state.frame_dir("f-1") / "notice").read_text()
        self.assertEqual(raw.split("\n")[1], "charter: gamma")

    def test_a_notice_is_gone_AT_its_expiry_and_not_a_moment_after(self):
        """The boundary, not just the direction. `<=` here would keep a notice for one
        more instant than it was given, which no operator could see — but the sweep is
        right that an unpinned boundary is an unpinned line, and the dwell is a half-open
        interval on purpose: `say(seconds=n)` means n seconds of it, not n plus a tick."""
        state.say("f-1", "charter: on the edge", seconds=30)
        at = state.notice_expiry("f-1")
        with mock.patch("time.time", return_value=at - 0.001):
            self.assertEqual(state.notice("f-1"), "charter: on the edge")
        with mock.patch("time.time", return_value=at):
            self.assertEqual(state.notice("f-1"), "")

    def test_a_hostile_fid_has_no_expiry_either(self):
        """`notice`'s containment guard has a twin in `notice_expiry`, and it is a
        separate line that needs a separate reason to exist: `frame_dir` answers `None`
        for a name `contain.child` refuses, and `None / "notice"` is a `TypeError` that
        the `(OSError, ValueError)` below would not catch — out of `panel._watch`'s run
        loop, which is the one place in this module that must never raise."""
        self.assertEqual(state.notice_expiry("../escape"), 0.0)

    def test_a_notice_whose_expiry_is_nan_is_not_live_forever(self):
        """`float("nan")` parses, and every comparison against a NaN is False — so a
        `now >= expiry` test reads False and the notice never expires. That is exactly the
        stuck row #727 is about, reachable through a corrupt file rather than a loop, so
        the comparison asks for the LIVE case and NaN falls out with every other
        degenerate value."""
        d = state.frame_dir("f-1", create=True)
        (d / "notice").write_text("nan\ncharter: forever\n")
        self.assertEqual(state.notice("f-1"), "")

    def test_a_notice_whose_expiry_is_infinite_is_still_bounded_by_nothing_but_reap(self):
        """The companion value: `inf` also parses, and unlike NaN it compares the way it
        reads. Pinned so the NaN guard above is understood as being about NaN rather than
        about "unusual floats" — an `inf` here is a notice that genuinely never expires,
        which nothing in charter writes and `reap` still removes."""
        d = state.frame_dir("f-1", create=True)
        (d / "notice").write_text("inf\ncharter: forever\n")
        self.assertEqual(state.notice("f-1"), "charter: forever")


if __name__ == "__main__":
    unittest.main()
