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

import sys
from types import SimpleNamespace
from unittest import mock

from charter import config, hooks, inflight, tui

from tests import _envguard
from tests._isolation import PersonaIso


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


class ATokenNamesOneRecord(unittest.TestCase):
    """`finish` can PICK a record or be TOLD one, and picking is a race.

    Name, kind and age is all a caller in another process can offer, and it is enough
    there: a process finishes what it started, so its own record is the only candidate it
    can have created. Two workers of one agent inside ONE process is a different shape —
    both glob, both filter, both select the identical file, one unlink wins, and the
    loser's record outlives its work by :data:`inflight.PRUNE_SECONDS`, drawn as running
    for the first half hour of it. `frame.actions.ActionRegistry.invoke` is the first
    caller of that shape, and `start` has always answered a token naming exactly one file.
    """

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self._orig = config.STATE_DIR
        config.STATE_DIR = Path(self._td.name)
        self.addCleanup(self._td.cleanup)
        self.addCleanup(lambda: setattr(config, "STATE_DIR", self._orig))

    def _stems(self) -> list[str]:
        return sorted(p.stem
                      for p in (config.STATE_DIR / "dispatch-inflight").glob("*.json"))

    def test_a_token_retires_the_record_it_names_and_not_the_pick(self):
        """The whole difference, stated without a thread: the search would take the
        OLDEST still-running record, and the second invocation's token is not it."""
        first = inflight.start("coder", kind=inflight.ACTION)
        second = inflight.start("coder", kind=inflight.ACTION)
        inflight.finish("coder", kind=inflight.ACTION, token=second)
        self.assertEqual(self._stems(), [first])

    def test_each_token_retires_its_own_and_two_leave_nothing(self):
        first = inflight.start("coder", kind=inflight.ACTION)
        second = inflight.start("coder", kind=inflight.ACTION)
        inflight.finish("coder", kind=inflight.ACTION, token=second)
        inflight.finish("coder", kind=inflight.ACTION, token=first)
        self.assertEqual(self._stems(), [])

    def test_a_token_whose_record_is_already_gone_is_harmless(self):
        token = inflight.start("coder", kind=inflight.ACTION)
        inflight.finish("coder", kind=inflight.ACTION, token=token)
        inflight.finish("coder", kind=inflight.ACTION, token=token)   # must not raise
        self.assertEqual(self._stems(), [])

    def test_a_token_that_is_not_a_bare_name_deletes_nothing(self):
        """A token is a file name the tracker wrote. One carrying a separator would make
        an unlink OUTSIDE the tracker's own directory a caller's string decides — the
        same reasoning `_safe_name` applies to an agent name, on the other half of the
        path."""
        outside = config.STATE_DIR / "evil.json"
        outside.write_text("{}")
        kept = inflight.start("coder", kind=inflight.ACTION)
        inflight.finish("coder", kind=inflight.ACTION, token="../evil")
        self.assertTrue(outside.exists(), "finish deleted outside its own directory")
        self.assertEqual(self._stems(), [kept])

    def test_no_token_still_picks_the_way_every_older_caller_expects(self):
        """The control: dispatch, clone and `gl-refresh` pass no token and must keep the
        behaviour they have — otherwise this is a change of `finish`, not an addition."""
        inflight.start("coder")
        inflight.finish("coder")
        self.assertEqual(self._stems(), [])


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
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

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


class RecordsSayWhatKindOfWorkTheyAre(unittest.TestCase):
    """#420. #387 promised the frame "a spinner while a dispatch, clone or `gl-refresh`
    runs" and shipped dispatches only, because `inflight.start` had exactly one caller.

    Wiring the other two in was blocked on this: the SAME records feed the
    dispatch-overlap nudge through `still_running`, which reads its answer back to an
    operator as a sentence — so a record named `clone` would have produced *"`x` writes
    code and `clone` are already running"*. Every test here is about which reader sees
    which kind, because that is the whole of the design.
    """

    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self._orig = config.STATE_DIR
        config.STATE_DIR = Path(self._td.name)
        self.addCleanup(self._td.cleanup)
        self.addCleanup(lambda: setattr(config, "STATE_DIR", self._orig))

    def test_a_clone_is_invisible_to_every_reader_that_names_what_is_running(self):
        """The three that would put the word in front of an operator: the nudge's own
        `still_running`, the aggregate `live` behind the status line's `⚡ N`, and
        `live_records`, which `statusline._inflight_by_persona` groups BY PERSONA — a
        clone landing there would invent a persona named after a repo."""
        inflight.start("iam-service", kind=inflight.CLONE)
        self.assertEqual(inflight.still_running(), [])
        self.assertEqual(inflight.live(), [])
        self.assertEqual(inflight.live_records(), [])

    def test_a_dispatch_is_still_visible_to_all_three(self):
        """The other direction, so the test above cannot be satisfied by a filter that
        hides everything."""
        inflight.start("coder")
        self.assertEqual(inflight.still_running(), ["coder"])
        self.assertEqual(inflight.live(), ["coder"])
        self.assertEqual([n for n, _, _ in inflight.live_records()], ["coder"])

    def test_asking_for_no_kind_at_all_sees_every_kind(self):
        """`kind=None` is what the frame's spinner asks for — it counts records and never
        names them, so a clone and a dispatch are both simply "running" there."""
        inflight.start("coder")
        inflight.start("iam-service", kind=inflight.CLONE)
        inflight.start("demo", kind=inflight.REFRESH)
        self.assertEqual(sorted(n for n, _, _ in inflight.live_records(kind=None)),
                         ["coder", "demo", "iam-service"])

    def test_the_default_is_dispatch_rather_than_everything(self):
        """**The default is the guard, not a convenience.** A reader that must not see a
        clone gets that by NOT asking, so the next kind somebody invents cannot leak into
        the nudge by being forgotten at one call site. Asserted as a property of the
        signature — every one of the three readers, unasked — rather than of one of them,
        because a fix that changed only the one under test is exactly the shape that
        would pass a single-reader check."""
        inflight.start("iam-service", kind=inflight.CLONE)
        for fn in (inflight.live, inflight.still_running, inflight.live_records):
            with self.subTest(fn=fn.__name__):
                self.assertEqual(list(fn()), [], f"{fn.__name__} defaulted to every kind")

    def test_a_record_written_before_the_field_existed_reads_as_a_dispatch(self):
        """A record on disk outlives the charter that wrote it — the tracker keeps one
        for `PRUNE_SECONDS` (a day), so an upgrade mid-dispatch is the ORDINARY case, not
        an edge one. A pre-#420 record has no `kind`, and it was a dispatch: reading it as
        anything else would drop a genuinely running peer out of the overlap nudge, which
        is the one thing that nudge exists to catch."""
        inflight.start("coder")
        p = _only_record()
        rec = json.loads(p.read_text())
        del rec["kind"]
        p.write_text(json.dumps(rec))
        self.assertEqual(inflight.still_running(), ["coder"])

    def test_a_kind_that_is_not_a_string_reads_as_a_dispatch_too(self):
        """Same reasoning, one step further: the file is on disk, so a truncated write or
        a hand edit can put anything there. A value that can never match a filter would
        hide a live record from every reader — including the nudge — which is a silent
        failure in the direction that matters. Degrading to `dispatch` shows it to the
        reader that would rather have a false positive than a miss."""
        inflight.start("coder")
        p = _only_record()
        p.write_text(json.dumps({"agent": "coder", "kind": 7, "ts": time.time()}))
        self.assertEqual(inflight.still_running(), ["coder"])

    def test_finish_retires_only_its_own_kind(self):
        """The file NAME carries the agent and not the kind, so a clone of a repo called
        `steward` and a dispatch to a persona called `steward` glob identically. Without
        a kind check inside `finish`, whichever finished first would retire the other's
        record — clearing a true one and leaving a false live one behind, which is
        exactly the state #308 built this tracker to make impossible."""
        inflight.start("steward")
        inflight.start("steward", kind=inflight.CLONE)
        inflight.finish("steward", kind=inflight.CLONE)
        self.assertEqual(inflight.still_running(), ["steward"])
        self.assertEqual([n for n, _, _ in inflight.live_records(kind=inflight.CLONE)],
                         [])

    def test_an_unreadable_record_is_never_the_one_finish_deletes(self):
        """`finish` deletes what the kind check admits, and a file charter cannot read is
        one it cannot claim belongs to this caller. Left alone here; `live_records`
        prunes it on its own horizon, which is where a stray belongs."""
        inflight.start("coder")
        p = _only_record()
        p.write_text("{not json")
        inflight.finish("coder")
        self.assertTrue(p.exists(), "finish deleted a record it could not identify")


class TheSpinnerFollowsEveryKindOfWork(PersonaIso, unittest.TestCase):
    """#420's actual deliverable: a clone or a `gl-refresh` moves the frame's spinner,
    and neither reaches the surfaces that would name it."""

    def _field(self) -> str:
        from charter.frame import slots
        with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((200, 24))):
            return tui.strip_ansi(slots._inflight_field())

    def test_a_clone_in_flight_moves_the_spinner(self):
        inflight.start("iam-service", kind=inflight.CLONE)
        self.assertIn("1 running", self._field())

    def test_a_gl_refresh_in_flight_moves_the_spinner(self):
        inflight.start("demo", kind=inflight.REFRESH)
        self.assertIn("1 running", self._field())

    def test_a_clone_and_a_dispatch_are_counted_together(self):
        """One count, not two fields: the row says how much work is in flight, and
        splitting it by kind would put a taxonomy on a status row nobody asked for."""
        inflight.start("coder")
        inflight.start("iam-service", kind=inflight.CLONE)
        self.assertIn("2 running", self._field())

    def test_the_panels_own_gate_agrees_with_what_the_row_will_draw(self):
        """`panel._running` decides whether the panel repaints often enough for the
        spinner to MOVE; `_inflight_field` decides what it says. If the gate stayed
        dispatch-only, a clone would leave the row showing a spinner frame frozen at
        whichever instant the last version bump happened to be — a still picture of an
        arbitrary frame, which is the exact thing `slots.SPINNER`'s own docstring says
        the clock-driven design exists to avoid."""
        from charter.frame import panel
        inflight.start("iam-service", kind=inflight.CLONE)
        self.assertEqual(panel._running(panel._new_inflight_cache()), 1)

    def test_nothing_in_flight_is_still_perfect_stillness(self):
        """The property #387 bought the whole gate for, re-asserted now that the gate
        admits more: widening WHAT animates must not widen WHEN."""
        from charter.frame import panel
        self.assertEqual(self._field(), "")
        self.assertEqual(panel._running(panel._new_inflight_cache()), 0)


class CloneAndRefreshActuallyRecordThemselves(PersonaIso, unittest.TestCase):
    """The wiring, not the mechanism: `inflight.start` having a second and third caller
    is the entire content of #420, and a `kind` field nothing ever writes would be a
    feature with no user."""

    def test_a_clone_holds_a_record_for_as_long_as_git_runs(self):
        """Observed from INSIDE the `git clone` call, which is the only moment it is
        observable — and the only moment it matters, since that is when an operator is
        looking at a frame wondering whether anything is happening. Asserted through the
        real `_clone_one`, with only `git` itself stubbed."""
        from charter import commands
        seen = []

        def fake_git(argv, **kw):
            seen.append(inflight.live_records(kind=inflight.CLONE))
            return SimpleNamespace(returncode=0, stdout="", stderr="")

        wd = config.WORKSPACES_DIR / "demo"
        wd.mkdir(parents=True, exist_ok=True)
        repo = {"name": "iam-service", "default_branch": "main",
                "http_url": "https://example.invalid/x/iam-service.git"}
        with mock.patch.object(commands, "_git", side_effect=fake_git), \
             mock.patch.object(commands, "_https_url",
                               return_value="https://example.invalid/x.git"), \
             mock.patch("charter.gitpolicy.apply"):
            res = commands._clone_one(repo, wd)
        self.assertEqual(res["status"], "ok")
        self.assertEqual([n for n, _, _ in seen[0]], ["iam-service"], seen)

    def test_the_clones_record_is_released_when_git_returns(self):
        from charter import commands
        wd = config.WORKSPACES_DIR / "demo"
        wd.mkdir(parents=True, exist_ok=True)
        repo = {"name": "iam-service", "default_branch": "main"}
        with mock.patch.object(commands, "_git",
                               return_value=SimpleNamespace(returncode=0, stdout="",
                                                            stderr="")), \
             mock.patch.object(commands, "_https_url",
                               return_value="https://example.invalid/x.git"), \
             mock.patch("charter.gitpolicy.apply"):
            commands._clone_one(repo, wd)
        self.assertEqual(inflight.live_records(kind=None), [])

    def test_a_clone_that_raises_still_releases_its_record(self):
        """`finally`, not a trailing call: a record left behind by an exception would
        spin an idle operator's frame until the presumed-dead threshold half an hour
        later, and `git` shelling out is exactly where an unexpected `OSError` lives."""
        from charter import commands
        wd = config.WORKSPACES_DIR / "demo"
        wd.mkdir(parents=True, exist_ok=True)
        repo = {"name": "iam-service", "default_branch": "main"}
        with mock.patch.object(commands, "_git", side_effect=OSError("no git")), \
             mock.patch.object(commands, "_https_url",
                               return_value="https://example.invalid/x.git"):
            with self.assertRaises(OSError):
                commands._clone_one(repo, wd)
        self.assertEqual(inflight.live_records(kind=None), [])

    def test_a_refused_clone_never_takes_a_record_at_all(self):
        """The record is taken after the destination and the URL are settled, so a repo
        charter refuses to clone leaves nothing behind to animate a frame over."""
        from charter import commands
        wd = config.WORKSPACES_DIR / "demo"
        wd.mkdir(parents=True, exist_ok=True)
        res = commands._clone_one({"name": "../escape", "default_branch": "main"}, wd)
        self.assertEqual(res["status"], "refused")
        self.assertEqual(inflight.live_records(kind=None), [])

    def test_gl_refresh_holds_a_record_while_it_fetches(self):
        """Observed from inside `glstate.refresh`, the call that actually goes to the
        forge. The workspace is the name, not a repo: one refresh covers every tree in
        it, and naming one of them would be a claim about which."""
        from charter import commands
        seen = []

        def fake_refresh(dirs):
            seen.append(inflight.live_records(kind=inflight.REFRESH))
            return {}

        with mock.patch("charter.workspace.repo_trees",
                        return_value=[config.WORKSPACES_DIR / "demo" / "r"]), \
             mock.patch("charter.worktree.dirs_for", return_value=[]), \
             mock.patch("charter.glstate.refresh", side_effect=fake_refresh):
            rc = commands.cmd_gl_refresh(SimpleNamespace(detach=False, workspace="demo"))
        self.assertEqual(rc, 0)
        self.assertEqual([n for n, _, _ in seen[0]], ["demo"], seen)
        self.assertEqual(inflight.live_records(kind=None), [],
                         "the refresh's record outlived the fetch")

    def test_the_detach_branch_records_nothing(self):
        """`--detach` returns before any work happens, in order to let the CHILD do it. A
        record taken there would clear before the child had begun, and the frame would
        blink rather than spin."""
        from charter import commands
        with mock.patch("charter.util.detach_self") as detach:
            rc = commands.cmd_gl_refresh(SimpleNamespace(detach=True, workspace="demo"))
        self.assertEqual(rc, 0)
        detach.assert_called_once()
        self.assertEqual(inflight.live_records(kind=None), [])


if __name__ == "__main__":
    unittest.main()
