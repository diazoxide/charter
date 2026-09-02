"""#810 group A: the thirteen `except OSError` clauses no test in the suite entered.

**The cause, in one line: nothing in the suite makes the filesystem refuse, so every one of
these clauses is never entered and the mutant that swaps the exception type changes nothing
that runs.** The sweep's `narrow-except` operator replaces the whole spec with
`ZeroDivisionError`; a clause that is never entered survives that, and thirteen did.

#810 poses this group as one decision rather than thirteen tests — *does charter test
filesystem failure at all?* — and notes that the sweep marks several of them `PLATFORM`,
meaning the clause may be unreachable on the machine the sweep ran on rather than untested.

**Both halves of that were checked before anything was written here, and the answer to the
first is yes.** `tests/test_a_quit_records_the_plane_before_it_kills
.TheCaptureIsBoundedAndNeverRaises` already enters two of `_capture_transcript`'s by
patching `config.private_mkdir` and `config.write_for`, and that is the seam: **the exact
call that would raise, patched, never a `chmod`.** A mode bit is advice to root, CI runs
some jobs as one, and a test whose premise the runner can ignore is a test that reports
green for the wrong reason.

**And none of the thirteen is `PLATFORM`-unreachable.** The marker is the sweep being
conservative about `OSError` as a NAME — its own note measures a pty read whose `OSError`
is dead on macOS and live on Linux — and every one of these is charter writing, reading,
scanning or unlinking a file **under its own state directory**. `ENOSPC`, `EACCES`,
`EROFS`, `EIO` and a directory removed underneath a scan are all reachable on every
platform charter runs on, and none of them depends on the operating system's opinion. So
this file is thirteen tests, not a measurement saying it cannot be twelve.

**Each case asserts the CONSEQUENCE, never "it did not raise".** Every one of these
functions documents what a failure costs — `False` from `reopen.write`, `[]` from
`plane_chats`, `0` from `prune_all`, a chat whose cwd was not recorded — and that sentence
is what is asserted. A test that only wrapped the call in `assertRaises`-free scaffolding
would stay green if the clause swallowed the failure AND left the caller a wrong answer.

**Two of them mask each other and are asked separately.** `inflight.prune_all` holds a
`glob` guard and an `unlink` guard in sequence: with the first swallowed the second is
never reached, so a mutant on the second looks equivalent when only the first is exercised.
One case makes the listing fail and one makes exactly one removal fail with the listing
intact.
"""

from __future__ import annotations

import os
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame, config, inflight, persona, util
from charter.frame import leave, reopen, state

from tests._isolation import PersonaIso


def _refuse(*_a, **_kw):
    """What a filesystem that will not do the thing raises. `EACCES` rather than a bare
    `OSError()`, because a guard narrowed to a SUBCLASS would still catch a bare one."""
    raise PermissionError(13, "refused")


class _FrameRoot(PersonaIso):
    def setUp(self):
        super().setUp()
        # Asserted, not assumed: every case below writes and deletes under the state
        # directory, and `PersonaIso` repointing it is the only thing between that and the
        # developer's own plane.
        self.assertIn("edm-test-", str(config.STATE_DIR))
        # **The process cwd is restored before the tmp plane is removed, and that ordering
        # is the whole of it.** `_reopen_one` does a real `os.chdir` into the recorded
        # directory, and one case here makes the chdir BACK fail on purpose — which is
        # exactly the branch under test. Left alone, the runner ends that case standing
        # inside a directory `PersonaIso` is about to `rmtree`, and every later
        # `os.getcwd()` in the whole run raises `FileNotFoundError`: measured on CI as
        # ~40 errors across nine unrelated modules, on all four interpreters. Cleanups run
        # LIFO and `PersonaIso` registered its teardown first, so this one runs before the
        # removal rather than after it.
        self.addCleanup(os.chdir, os.getcwd())
        config.private_mkdir(state._root())

    def chat(self, **kw):
        base = dict(chat="alpha.1", workspace="alpha", persona="", harness="claude-code",
                    cwd=str(config.ROOT), resume="", transcript="", active=True)
        base.update(kw)
        return reopen.Chat(**base)

    def frame(self, *chats, workspace="alpha"):
        return reopen.Frame(workspace=workspace, chats=tuple(chats))


class RecordingThePlaneSaysSoWhenItCannot(_FrameRoot):
    """`reopen.write`'s two clauses. What a failure costs is stated in its docstring: the
    operator is told the plane was not recorded and decides whether to quit anyway — so
    `False` is the assertion, and a manifest that is still the PREVIOUS one is the other
    half, because a half-written manifest is a plane half-restored."""

    def test_a_frame_root_that_cannot_be_made_records_nothing(self):
        with mock.patch.object(config, "private_mkdir", side_effect=_refuse):
            self.assertFalse(reopen.write([self.frame(self.chat())], focus="alpha"))

        self.assertIsNone(reopen.read())

    def test_a_write_that_cannot_land_leaves_the_previous_manifest_whole(self):
        self.assertTrue(reopen.write([self.frame(self.chat(chat="alpha.1"))],
                                     focus="alpha"))

        with mock.patch.object(config, "write_for", side_effect=_refuse):
            self.assertFalse(reopen.write([self.frame(self.chat(chat="alpha.2"))],
                                          focus="beta"))

        kept = reopen.read()
        self.assertIsNotNone(kept)
        self.assertEqual(kept.focus, "alpha")
        self.assertEqual(kept.frames[0].chats[0].chat, "alpha.1",
                         "a reader sees the previous manifest whole or this one whole")

    def test_a_rename_that_cannot_complete_records_nothing(self):
        """The second half of the same clause: `config.write_for` landed and `os.replace`
        refused. Its own case because the temp file exists at that point and the earlier
        one never gets that far."""
        with mock.patch.object(reopen.os, "replace", side_effect=_refuse):
            self.assertFalse(reopen.write([self.frame(self.chat())], focus="alpha"))

        self.assertIsNone(reopen.read())

    def test_a_value_json_cannot_serialise_records_nothing(self):
        """`(OSError, TypeError, ValueError)` is three names, and narrowing it to the first
        would leave a `TypeError` from `json.dumps` taking the quit down instead of costing
        one manifest. A recorded field is whatever the caller put in the record."""
        with self.assertRaises(TypeError):
            reopen.json.dumps({"x": object()})

        self.assertFalse(reopen.write([self.frame(self.chat(active=object()))],
                                      focus="alpha"))
        self.assertIsNone(reopen.read())


class ForgettingTheManifestNeverFailsAReopen(_FrameRoot):
    def test_a_manifest_that_cannot_be_removed_costs_a_duplicate_tab_and_nothing_else(self):
        """`reopen.forget`'s docstring: *a manifest that could not be removed costs a
        duplicated tab the operator can close, and a reopen that had already relaunched
        every harness must not report failure over a file.* So the assertion is that the
        call returns — and that the manifest is still readable, which is the duplicate tab
        the sentence is about."""
        reopen.write([self.frame(self.chat())], focus="alpha")

        with mock.patch.object(Path, "unlink", side_effect=_refuse):
            self.assertIsNone(reopen.forget())

        self.assertIsNotNone(reopen.read())


class PruningTranscriptsNeverFailsAQuit(_FrameRoot):
    """`prune_transcripts`' two clauses — the listing, and one removal.

    Asked separately for `inflight.prune_all`'s reason: with the listing swallowed there is
    no loop, so a mutant on the removal guard is masked by a case that only breaks the
    scan.
    """

    def _transcripts(self, *chats) -> list[Path]:
        made = []
        for c in chats:
            p = reopen.transcript_path(c)
            config.write_for(p, f"{c} was on screen\n")
            made.append(p)
        return made

    def test_a_frame_root_that_cannot_be_scanned_removes_nothing(self):
        kept, gone = self._transcripts("alpha.1", "alpha.2")

        with mock.patch.object(reopen.os, "scandir", side_effect=_refuse):
            self.assertIsNone(reopen.prune_transcripts({"alpha.1"}))

        self.assertTrue(kept.exists())
        self.assertTrue(gone.exists(), "nothing was swept, rather than some of it")

    def test_one_transcript_that_cannot_be_removed_does_not_strand_the_rest(self):
        kept, stuck, sweepable = self._transcripts("alpha.1", "alpha.2", "alpha.3")
        real = reopen.os.unlink

        def refuse_one(path, *a, **kw):
            if os.fspath(path) == str(stuck):
                _refuse()
            return real(path, *a, **kw)

        with mock.patch.object(reopen.os, "unlink", side_effect=refuse_one):
            self.assertIsNone(reopen.prune_transcripts({"alpha.1"}))

        self.assertTrue(kept.exists(), "the one the manifest still names")
        self.assertTrue(stuck.exists(), "the one that refused")
        self.assertFalse(sweepable.exists(), "the loop carried on past the refusal")


class ScanningThePlaneAnswersEmptyRatherThanRaising(_FrameRoot):
    def test_a_frame_root_that_cannot_be_scanned_is_no_chats(self):
        """`leave.plane_chats` is called by `cmd_quit` BEFORE anything is killed. `[]` is
        the documented answer, and it is the safe one: a quit that cannot see the plane
        records nothing and stops nothing, rather than half of each."""
        state.frame_dir("alpha.1", create=True)
        self.assertEqual(leave.plane_chats(), ["alpha.1"])

        with mock.patch.object(leave.os, "scandir", side_effect=_refuse):
            self.assertEqual(leave.plane_chats(), [])


class AChatsOwnRecordsFailQuietly(_FrameRoot):
    """`state.record_cwd` and `state.record_closed` — two of the writers whose whole
    contract is *never raises*, asserted through the reader on the other side."""

    def test_a_cwd_that_cannot_be_written_leaves_the_chat_with_none(self):
        state.frame_dir("alpha.1", create=True)

        with mock.patch.object(config, "write_for", side_effect=_refuse):
            self.assertIsNone(state.record_cwd("alpha.1", "/some/where"))

        self.assertIsNone(state.chat_cwd("alpha.1"))

    def test_a_cwd_whose_rename_refuses_leaves_the_chat_with_none(self):
        """The same clause, entered at `os.replace` instead of at the write — the temp file
        landed and the atomic move did not, which is the half that leaves debris."""
        state.frame_dir("alpha.1", create=True)

        with mock.patch.object(state.os, "replace", side_effect=_refuse):
            self.assertIsNone(state.record_cwd("alpha.1", "/some/where"))

        self.assertIsNone(state.chat_cwd("alpha.1"))

    def test_a_close_marker_that_cannot_be_written_leaves_the_chat_open(self):
        """`record_closed`'s docstring states this cost rather than guarding it: the window
        is killed either way, and a quit landing before `reap` collects the directory would
        record the chat as open and a reopen would bring it back. That is the assertion."""
        state.frame_dir("alpha.1", create=True)

        with mock.patch.object(config, "write_for", side_effect=_refuse):
            self.assertIsNone(state.record_closed("alpha.1"))

        self.assertFalse(state.was_closed("alpha.1"))


class TheInflightTrackerNeverBreaksAQuit(_FrameRoot):
    """`inflight.prune_all`'s pair, and the masking #810 names explicitly.

    *"Two guards in sequence, so neither is safe to call equivalent on its own."* With the
    `glob` swallowed there is no loop at all, so a mutant on the `unlink` guard survives
    every case that only breaks the listing. One case each.
    """

    def _records(self, *agents) -> list[Path]:
        made = []
        for a in agents:
            inflight.start(a, kind="Task")
            made.append(a)
        return [p for p in inflight._dir().glob("*.json")]

    def test_a_tracker_directory_that_cannot_be_listed_prunes_nothing(self):
        self._records("agent-a", "agent-b")

        with mock.patch.object(Path, "glob", side_effect=_refuse):
            self.assertEqual(inflight.prune_all(), 0)

        self.assertEqual(len(list(inflight._dir().glob("*.json"))), 2,
                         "nothing was pruned, and nothing raised into the quit")

    def test_one_record_that_cannot_be_removed_does_not_strand_the_rest(self):
        records = self._records("agent-a", "agent-b", "agent-c")
        self.assertEqual(len(records), 3)
        stuck = records[0]
        real = Path.unlink

        def refuse_one(self_, *a, **kw):
            if self_ == stuck:
                _refuse()
            return real(self_, *a, **kw)

        with mock.patch.object(Path, "unlink", refuse_one):
            self.assertEqual(inflight.prune_all(), 2,
                             "the count is what was removed, not what was tried")

        self.assertTrue(stuck.exists())
        self.assertEqual(len(list(inflight._dir().glob("*.json"))), 1)


class RestoringAChatsRecordsNeverFailsTheRelaunch(_FrameRoot):
    """`_restore_recorded_chat`'s two clauses. Its docstring names both costs: *a persona
    that could not be pointed at leaves the chat on the plane's default, which is visible on
    its own panel; a transcript that could not be moved leaves the row with nothing to
    offer. Neither is worth failing a relaunch over.*"""

    def test_a_persona_pointer_that_cannot_be_written_still_moves_the_transcript(self):
        """The order matters and the assertion is the second half: the persona is attempted
        first, so a clause that let its failure through would take the transcript with it.
        `(OSError, ValueError)` is two names — `persona.set_active` answers `ValueError`
        for a name it will not take — so both are entered."""
        old = reopen.transcript_path("alpha.9")
        config.write_for(old, "what was on screen\n")
        rec = self.chat(chat="alpha.9", persona="steward")

        for boom in (_refuse, lambda *a, **k: (_ for _ in ()).throw(ValueError("no"))):
            config.write_for(old, "what was on screen\n")
            with mock.patch.object(persona, "set_active",
                                   side_effect=boom):
                self.assertIsNone(
                    commands_frame._restore_recorded_chat(rec, "alpha.1"))
            self.assertEqual(reopen.transcript_path("alpha.1").read_text(),
                             "what was on screen\n")
            reopen.transcript_path("alpha.1").unlink()

    def test_a_transcript_that_cannot_be_moved_leaves_the_chat_relaunched(self):
        config.write_for(reopen.transcript_path("alpha.9"), "what was on screen\n")
        rec = self.chat(chat="alpha.9", persona="steward")
        self.make_persona("steward")

        with mock.patch.object(commands_frame.os, "replace", side_effect=_refuse):
            self.assertIsNone(commands_frame._restore_recorded_chat(rec, "alpha.1"))

        self.assertFalse(reopen.transcript_path("alpha.1").exists())
        self.assertEqual(persona.for_session("alpha.1"), "steward",
                         "the half that landed is kept")


class AReopenThatCannotStandSomewhereSaysSo(_FrameRoot):
    """`_reopen_one`'s two `os.chdir` clauses, and they are opposite in kind: one is a
    refusal the operator is told about, and one is a failure on the way BACK that nobody
    can act on."""

    def _reopen(self, chat):
        """`_reopen_one` with the launcher standing in, so what is measured is the
        directory the launcher was called in and what came back — never tmux."""
        said: list[str] = []
        calls: list = []

        def launch(args):
            calls.append(os.getcwd())
            args.reopening.fid = "alpha.1"
            return 0

        with mock.patch.object(util, "warn", side_effect=said.append), \
                mock.patch.object(commands_frame, "cmd_launch", side_effect=launch):
            out = commands_frame._reopen_one(chat)
        return out, said, calls

    def test_a_directory_charter_cannot_enter_is_reported_and_not_relaunched(self):
        here = os.getcwd()
        with mock.patch.object(commands_frame.os, "chdir", side_effect=_refuse), \
                mock.patch.object(commands_frame.os.path, "isdir", return_value=True):
            out, said, calls = self._reopen(self.chat(cwd="/some/where"))

        self.assertIsNone(out, "the chat is not reopened somewhere else instead")
        self.assertEqual(calls, [], "the launcher was never reached")
        self.assertTrue(any("cannot enter" in s for s in said))
        self.assertEqual(os.getcwd(), here)

    def test_a_launcher_that_cannot_get_back_still_reports_the_chat(self):
        """The `finally`'s own clause. Nothing the operator can do about it and nothing to
        say, so it is swallowed — but the reopen must still return its `Reopening`, because
        the chat DID come back and a `None` here would report it as lost."""
        real = os.chdir
        seen = []

        def chdir_out_only(path):
            seen.append(str(path))
            if len(seen) > 1:
                _refuse()
            return real(path)

        with mock.patch.object(commands_frame.os, "chdir", side_effect=chdir_out_only):
            out, said, calls = self._reopen(self.chat(cwd=str(config.ROOT)))

        self.assertIsNotNone(out)
        self.assertEqual(out.fid, "alpha.1")
        self.assertEqual(len(calls), 1)
        # And the cost, stated rather than tidied away: the process really is left in the
        # recorded directory. Nothing in `_reopen_one` can fix that — the `finally` already
        # tried — so the honest assertion is that it happened, and `setUp`'s cleanup is
        # what puts the RUNNER back before the tmp plane is removed.
        self.assertEqual(os.getcwd(), str(config.ROOT.resolve()))


if __name__ == "__main__":
    unittest.main()
