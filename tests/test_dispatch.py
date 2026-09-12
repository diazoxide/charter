"""The committed dispatch tally — the instrument that makes a never-used persona visible.

Guards the three properties the store is built on: it records counts+dates and NEVER the
prompt; parallel writers don't clobber each other; and the hook only fires for real
sub-agent dispatches.
"""
from __future__ import annotations

import json
import os
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest import mock

from tests._isolation import PersonaIso, PlaneIso, run_hook
from charter import commands, config, dispatch, hooks


class TestDispatchStore(PersonaIso):
    def test_record_then_tally(self):
        dispatch.record("devops")
        dispatch.record("devops")
        dispatch.record("qa")
        self.assertEqual(dispatch.tally()["devops"], 2)
        self.assertEqual(dispatch.tally()["qa"], 1)

    def test_never_dispatched_reads_as_zero_not_missing(self):
        dispatch.record("devops")
        self.assertEqual(dispatch.tally()["nx-code-reviewer"], 0)
        self.assertIsNone(dispatch.last_seen("nx-code-reviewer"))

    def test_line_holds_counts_and_dates_only(self):
        """The whole reason this store needs no secret-scan: no prompt ever reaches it."""
        p = dispatch.record("devops")
        row = json.loads(p.read_text().splitlines()[0])
        self.assertEqual(set(row), {"ts", "agent"})
        self.assertEqual(row["agent"], "devops")

    def test_days_window_excludes_older_rows(self):
        old = datetime.now(timezone.utc) - timedelta(days=40)
        dispatch.record("devops", when=old)
        dispatch.record("qa")
        self.assertEqual(dispatch.tally(days=14), {"qa": 1})

    def test_generic_agents_are_tallied_too(self):
        """Dropping generic dispatches would hide the persona-vs-generic ratio — the one
        number that exposed 42 reviews going to general-purpose."""
        for _ in range(3):
            dispatch.record("general-purpose")
        dispatch.record("devops")
        self.assertEqual(dispatch.generic_share(), (3, 4))

    def test_parallel_appends_do_not_clobber(self):
        """A fan-out of sub-agents finishing together must not lose lines."""
        import threading
        threads = [threading.Thread(target=dispatch.record, args=(f"a{i%3}",)) for i in range(30)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        self.assertEqual(sum(dispatch.tally().values()), 30)

    def test_store_dir_is_not_mistaken_for_a_persona(self):
        from charter import persona
        dispatch.record("devops")
        self.assertNotIn(dispatch.DIR_NAME, persona.list_personas())


class TestDispatchHook(PlaneIso):
    def _run(self, payload):
        with mock.patch.object(hooks, "_commit_dispatch"):  # don't touch git in tests
            return run_hook(hooks.posttooluse_dispatch, payload)

    def test_task_dispatch_is_recorded(self):
        self._run({"tool_name": "Task", "tool_input": {"subagent_type": "devops"}})
        self.assertEqual(dispatch.tally()["devops"], 1)

    def test_other_tools_are_ignored(self):
        self._run({"tool_name": "Bash", "tool_input": {"command": "ls"}})
        self.assertEqual(sum(dispatch.tally().values()), 0)

    def test_missing_subagent_type_is_ignored(self):
        self._run({"tool_name": "Task", "tool_input": {"prompt": "do a thing"}})
        self.assertEqual(sum(dispatch.tally().values()), 0)

    def test_hook_never_raises_when_the_store_is_unwritable(self):
        with mock.patch.object(dispatch, "record", side_effect=OSError("boom")):
            self.assertIsNone(self._run({"tool_name": "Task",
                                         "tool_input": {"subagent_type": "devops"}}))


class TestAHandoffRowIsWrittenLikeEveryOtherRow(PersonaIso):
    """`record_handoff`'s three writer guards — the ones this store has always had and
    which `charter handoff` brought a fourth copy of (chat-handoff plan, Task 2).

    A tally must never break a turn, and it must never be the thing that publishes
    somewhere it was aimed at. Both are properties of the write, not of the row.
    """

    def _row(self):
        rows = [o for o in dispatch._read_all() if o.get("event") == dispatch.HANDOFF]
        return rows

    def test_a_row_lands_and_says_only_the_four_things(self):
        p = dispatch.record_handoff(placement="elsewhere", created=False)
        self.assertIsNotNone(p)
        self.assertEqual([set(o) for o in self._row()],
                         [{"created", "event", "placement", "ts"}])

    def test_the_month_file_is_written_at_0644(self):
        """Committed and shared, so it is readable — and it is charter's to create, so the
        umask does not get to decide. The same mode the other three writers here pass."""
        p = dispatch.record_handoff(placement="here", created=True)
        self.assertEqual(p.stat().st_mode & 0o777, 0o644)

    def test_a_month_file_that_is_a_link_out_of_the_plane_is_refused(self):
        """`contain.write_refusal`'s case, at a FIXED name an attacker needs no guess for:
        the row would be appended wherever the link points, outside the plane."""
        outside = Path(self.tmp).parent / "outside-the-plane.jsonl"
        target = dispatch.path_for()
        target.parent.mkdir(parents=True, exist_ok=True)
        target.symlink_to(outside)
        self.addCleanup(lambda: outside.exists() and outside.unlink())

        self.assertIsNone(dispatch.record_handoff(placement="here", created=False))
        self.assertFalse(outside.exists(), "the row was written through the link")

    def test_a_store_that_cannot_be_written_is_not_an_exception(self):
        """The tally is bookkeeping: it may miss a row, it may not break the turn it is
        counting. Measured by taking write permission off the directory it appends in."""
        if os.geteuid() == 0:
            self.skipTest("root ignores the mode, so this says nothing about the guard")
        d = dispatch._dir()
        d.mkdir(parents=True, exist_ok=True)
        d.chmod(0o500)
        self.addCleanup(d.chmod, 0o700)
        self.assertIsNone(dispatch.record_handoff(placement="here", created=False))


class TestCommitDispatchPosture(unittest.TestCase):
    """`_commit_dispatch` is the one gate whose failure publishes a memory line without
    anyone typing a command — it must honour `config.MEMORY_SHARE` exactly like the other
    two reactive paths (`commands.commit_memory_reactive`, `cmd_workspace_autosave`).
    Same shape as `TestReactiveCommitHonoursPosture` (test_memory_share.py) and
    `TestAutosaveGating` (test_workspace_enforcement.py): git is mocked, only the posture
    wiring is under test."""

    def setUp(self):
        import tempfile
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-dispatch-commit-"))
        self._orig = {k: getattr(config, k) for k in ("ROOT", "STATE_DIR", "MEMORY_SHARE")}
        config.ROOT = self.tmp
        config.STATE_DIR = self.tmp / ".charter"
        self.path = self.tmp / "personas" / "_dispatch" / "log.jsonl"
        self.path.parent.mkdir(parents=True)
        self.path.write_text('{"ts": "x", "agent": "devops"}\n')
        self.addCleanup(self._restore)

    def _restore(self):
        import shutil
        for k, v in self._orig.items():
            setattr(config, k, v)
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _run(self, share):
        config.MEMORY_SHARE = share
        with mock.patch.object(commands, "commit_push") as cp:
            hooks._commit_dispatch(self.path, "devops")
        return cp

    def test_local_never_acquires_the_lock(self):
        """The earliest observable side effect, per the reviewer's trace: `_commit_dispatch`
        returns before it creates `dispatch-commit.lock` at all. Asserting only that
        `commit_push` wasn't called would still pass even if an earlier step (mkdir/open/
        flock) had already leaked under `local` — this pins the return to before that."""
        cp = self._run("local")
        cp.assert_not_called()
        self.assertFalse((config.STATE_DIR / "dispatch-commit.lock").exists())
        self.assertFalse(config.STATE_DIR.exists(),
                         "local must not even create STATE_DIR's dispatch-commit.lock parent")

    def test_commit_records_but_does_not_push(self):
        cp = self._run("commit")
        cp.assert_called_once()
        self.assertTrue(cp.call_args.kwargs.get("no_push"), "`commit` must not publish")

    def test_push_publishes_immediately(self):
        cp = self._run("push")
        cp.assert_called_once()
        self.assertFalse(cp.call_args.kwargs.get("no_push"),
                         "`push` is today's umbrella behaviour: publish right away")

    def test_unrecognised_value_behaves_like_local(self):
        """`config.MEMORY_SHARE` is always pre-clamped through `instance.share_of` — but
        this gate must not itself depend on that. Given a value outside the three known
        modes (a typo, a test, a future caller setting the attribute directly), it must
        fall back to `local`'s exact behaviour: no commit attempted at all, not fall
        through the if/elif chain into an immediate FOREGROUND push (worse than either
        sibling path, which degrade gracefully)."""
        cp = self._run("puhs")
        cp.assert_not_called()
        self.assertFalse((config.STATE_DIR / "dispatch-commit.lock").exists())
        self.assertFalse(config.STATE_DIR.exists(),
                         "an unrecognised posture must not even create STATE_DIR's lock parent")


if __name__ == "__main__":
    unittest.main()
