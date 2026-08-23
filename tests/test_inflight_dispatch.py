"""Overlapping dispatches into one working tree — the signal the tally can't give.

`personas/_dispatch/` records a dispatch when it **finishes**, so two five minutes
apart sequentially look exactly like two that overlapped. `inflight` records the
**start**, so overlap is observable.

The nudge is deliberately narrow: it fires only when the incoming persona declares
`dispatch-isolation: worktree`. A read-only fan-out overlapping is normal, and a
warning on it would train people to ignore the warning — which is the failure mode
that matters more than the one it reports.

It never denies. `isolation` is the caller's Agent-tool parameter and charter
cannot set it; saying so at dispatch time is the whole of what's available.
"""

from __future__ import annotations

import io
import json
import os
import time
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from tempfile import TemporaryDirectory

from charter import config, hooks, inflight


def _age(path: Path, seconds: float) -> Path:
    """Backdate one record by *seconds* — both its ``ts`` and its mtime.

    The two thresholds read different fields on purpose (the prune horizon reads mtime
    before parsing, so a corrupt stray is still cleaned; presumed-dead reads the ``ts``
    the age is rendered from, so the mark and the number can never disagree). A fixture
    that moved only one of them could therefore pass a test that a real record fails.
    """
    when = time.time() - seconds
    try:
        rec = json.loads(path.read_text())
        rec["ts"] = when
        path.write_text(json.dumps(rec))
    except (OSError, ValueError):
        pass
    os.utime(path, (when, when))
    return path


def _only_record() -> Path:
    return next((config.STATE_DIR / "dispatch-inflight").glob("*.json"))


class _Stdin:
    """Feed a payload to a handler that reads stdin."""

    def __init__(self, payload):
        self.payload = payload

    def __enter__(self):
        self._orig = hooks._read_stdin
        hooks._read_stdin = lambda: self.payload
        return self

    def __exit__(self, *a):
        hooks._read_stdin = self._orig


def _dispatch_payload(agent: str) -> dict:
    return {"tool_name": "Task", "tool_input": {"subagent_type": agent}}


class InflightStore(unittest.TestCase):
    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self._orig = config.STATE_DIR
        config.STATE_DIR = Path(self._td.name)
        self.addCleanup(self._td.cleanup)
        self.addCleanup(lambda: setattr(config, "STATE_DIR", self._orig))

    def test_nothing_in_flight_initially(self):
        self.assertEqual(inflight.live(), [])

    def test_start_then_live(self):
        inflight.start("coder")
        self.assertEqual(inflight.live(), ["coder"])

    def test_finish_clears_one_record(self):
        inflight.start("coder")
        inflight.finish("coder")
        self.assertEqual(inflight.live(), [])

    def test_two_starts_need_two_finishes(self):
        """A repeat dispatch of one persona must not be retired by a single finish."""
        inflight.start("coder")
        inflight.start("coder")
        inflight.finish("coder")
        self.assertEqual(inflight.live(), ["coder"])

    def test_a_record_past_the_prune_horizon_is_removed(self):
        """A killed process leaves a record behind; it must not accumulate forever."""
        inflight.start("coder")
        p = _age(_only_record(), inflight.PRUNE_SECONDS + 60)
        self.assertEqual(inflight.live(), [])
        self.assertFalse(p.exists(), "a stray should be removed, not just ignored")

    def test_a_fresh_record_is_not_pruned(self):
        inflight.start("coder")
        self.assertEqual(inflight.live(), ["coder"])


class PresumedDead(unittest.TestCase):
    """The record outliving every reasonable expectation is the *most* interesting thing
    this tracker can hold, and deleting it rendered it as nothing at all — "presumed dead"
    and "never happened" were indistinguishable, irreversibly (#308).

    So one threshold became two: past `PRESUMED_DEAD_SECONDS` a record is still returned,
    flagged; only past `PRUNE_SECONDS` does it go away.
    """

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self._orig = config.STATE_DIR
        config.STATE_DIR = Path(self._td.name)
        self.addCleanup(self._td.cleanup)
        self.addCleanup(lambda: setattr(config, "STATE_DIR", self._orig))

    def test_a_fresh_record_is_not_flagged(self):
        inflight.start("coder")
        self.assertEqual([r[2] for r in inflight.live_records()], [False])

    def test_a_record_past_the_threshold_is_kept_and_flagged(self):
        inflight.start("coder")
        p = _age(_only_record(), inflight.PRESUMED_DEAD_SECONDS + 60)
        recs = inflight.live_records()
        self.assertEqual([(r[0], r[2]) for r in recs], [("coder", True)])
        self.assertTrue(p.exists(), "a presumed-dead record must survive being read")

    def test_reading_twice_does_not_consume_it(self):
        """The old deletion was irreversible: the first render after the threshold was
        the last one that could ever mention it."""
        inflight.start("coder")
        _age(_only_record(), inflight.PRESUMED_DEAD_SECONDS + 60)
        inflight.live_records()
        self.assertEqual([r[2] for r in inflight.live_records()], [True])

    def test_the_age_keeps_climbing_past_the_threshold(self):
        inflight.start("coder")
        _age(_only_record(), 3 * 60 * 60)
        started = inflight.live_records()[0][1]
        self.assertAlmostEqual(started, time.time() - 3 * 60 * 60, delta=5)

    def test_live_still_names_a_presumed_dead_dispatch(self):
        """`_session_news` counts what `live()` returns, and the strip's aggregate counts
        every record the tracker holds — the chips are where the distinction lives."""
        inflight.start("coder")
        _age(_only_record(), inflight.PRESUMED_DEAD_SECONDS + 60)
        self.assertEqual(inflight.live(), ["coder"])

    def test_the_prune_horizon_is_the_far_one(self):
        self.assertGreater(inflight.PRUNE_SECONDS, inflight.PRESUMED_DEAD_SECONDS)

    def test_finish_retires_a_live_record_before_a_presumed_dead_one(self):
        """`finish` retires the OLDEST record for a name. Once the oldest can be a
        presumed-dead stray, that rule alone would clear the stuck record and leave the
        one that just finished behind — re-creating #308 through the back door."""
        inflight.start("coder")
        stuck = _age(_only_record(), inflight.PRESUMED_DEAD_SECONDS + 60)
        inflight.start("coder")
        inflight.finish("coder")
        self.assertTrue(stuck.exists(), "the stuck record must survive a peer finishing")
        self.assertEqual([r[2] for r in inflight.live_records()], [True])

    def test_finish_retires_a_presumed_dead_record_when_nothing_else_is_live(self):
        """A genuinely long dispatch does finish eventually, and when it does its record
        must go — otherwise every run over the threshold leaks one."""
        inflight.start("coder")
        _age(_only_record(), inflight.PRESUMED_DEAD_SECONDS + 60)
        inflight.finish("coder")
        self.assertEqual(inflight.live(), [])

    def test_finish_still_retires_the_oldest_of_several_live_records(self):
        inflight.start("coder")
        first = _age(_only_record(), 5 * 60)
        inflight.start("coder")
        inflight.finish("coder")
        self.assertFalse(first.exists())
        self.assertEqual(len(inflight.live_records()), 1)

    def test_an_agent_name_with_path_characters_is_made_safe(self):
        inflight.start("../../etc/passwd")
        files = list((config.STATE_DIR / "dispatch-inflight").glob("*.json"))
        self.assertEqual(len(files), 1)
        self.assertNotIn("/", files[0].name)

    def test_finish_on_an_unknown_agent_is_harmless(self):
        inflight.finish("never-started")   # must not raise

    def test_start_with_no_agent_is_a_noop(self):
        self.assertIsNone(inflight.start(""))
        self.assertEqual(inflight.live(), [])


class DispatchNudge(unittest.TestCase):
    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        root = Path(self._td.name)
        # `config.use`, not three named attributes. The nudge these cases drive calls
        # `_trace("dispatch-ask", data.get("session_id"))`, and `_run`'s payload carries no
        # session id — so the row fell through to the ambient `$CHARTER_SESSION_ID` and
        # landed in the OPERATOR's live trace bucket, which is what `charter trace
        # --summary` counts (#372). `PERSONA_STATE_DIR` was the attribute this list missed;
        # `config.DERIVED` exists so a fixture cannot miss one.
        #
        # `charter.toml` first, because `use` re-derives HAS_CONTROL_PLANE and the
        # hand-rolled patch left the real plane's `True` standing.
        (root / "charter.toml").write_text("schema = 1\n")
        self._orig = config.use(root)
        self.addCleanup(self._td.cleanup)
        self.addCleanup(lambda: config.restore(self._orig))

        self._persona("coder", "dispatch-isolation: worktree\n")
        self._persona("explorer", "")

    def _persona(self, name: str, extra: str) -> None:
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text(
            f"---\nname: {name}\nrole: {name.title()}\nvault: {name}\n"
            f"delegate-when: things\n{extra}---\n\n# {name.title()}\nTask: x.\n")

    def _run(self, agent: str) -> str:
        buf = io.StringIO()
        with _Stdin(_dispatch_payload(agent)), redirect_stdout(buf):
            rc = hooks.pretooluse_dispatch()
        self.assertEqual(rc, 0, "a nudge must never break a turn")
        return buf.getvalue()

    def test_first_dispatch_is_silent(self):
        self.assertEqual(self._run("coder").strip(), "")

    def test_overlapping_code_writer_is_nudged(self):
        self._run("coder")                      # now in flight
        out = self._run("coder")
        spec = json.loads(out)["hookSpecificOutput"]
        self.assertEqual(spec["permissionDecision"], "ask", "must nudge, never deny")
        self.assertIn("isolation: worktree", spec["permissionDecisionReason"])

    def test_a_read_only_persona_overlapping_is_silent(self):
        """The false-positive guard: only a declared code-writer is worth warning about."""
        self._run("coder")
        self.assertEqual(self._run("explorer").strip(), "")

    def test_silent_again_once_the_peer_finishes(self):
        self._run("coder")
        inflight.finish("coder")
        self.assertEqual(self._run("coder").strip(), "")

    def test_a_presumed_dead_peer_does_not_nudge(self):
        """The nudge claims a peer *is already running*, and past the presumed-dead
        threshold charter no longer knows that. Keeping the record so a stuck dispatch
        stays visible (#308) must not turn into a nag that outlives the process by a day
        — the window this asserts on is the one the old TTL pruning gave it.
        """
        self._run("coder")
        _age(_only_record(), inflight.PRESUMED_DEAD_SECONDS + 60)
        self.assertEqual(self._run("coder").strip(), "")

    def test_an_unknown_agent_does_not_raise(self):
        self._run("coder")
        self.assertEqual(self._run("no-such-persona").strip(), "")

    def test_non_dispatch_tools_are_ignored(self):
        buf = io.StringIO()
        with _Stdin({"tool_name": "Bash", "tool_input": {"command": "ls"}}), redirect_stdout(buf):
            self.assertEqual(hooks.pretooluse_dispatch(), 0)
        self.assertEqual(buf.getvalue().strip(), "")

    def test_empty_payload_is_ignored(self):
        buf = io.StringIO()
        with _Stdin({}), redirect_stdout(buf):
            self.assertEqual(hooks.pretooluse_dispatch(), 0)
        self.assertEqual(buf.getvalue().strip(), "")


class ManifestWiring(unittest.TestCase):
    def test_the_handler_is_registered(self):
        self.assertIn("pretooluse-dispatch", hooks._HANDLERS)

    def test_the_manifest_declares_it_against_task_and_agent(self):
        doc = json.loads((Path(__file__).resolve().parent.parent /
                          "hooks" / "hooks.json").read_text())
        cmds = [(g.get("matcher"), h["command"])
                for g in doc["hooks"]["PreToolUse"] for h in g["hooks"]]
        match = [m for m, c in cmds if "pretooluse-dispatch" in c]
        self.assertEqual(match, ["Task|Agent"],
                         "must match dispatch tools only — never every Bash call")


if __name__ == "__main__":
    unittest.main()
