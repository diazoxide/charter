"""Status-line context + prompt-cache gauge.

Token efficiency in Claude Code is decided mostly by *prompt caching*: the unchanged request
prefix is served at ~10% of the input rate, so what matters is the share of input READ from
cache vs re-written. Sustained high cache *creation* means the prefix keeps changing (model or
effort switch, MCP server connect/disconnect, plugin toggle, /compact). This gauge surfaces
that live, so token work is measured instead of argued.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

from unittest import mock

from charter import config, statusline
from tests import _envguard
from tests._isolation import PersonaIso, isolate_state_dir, pin_update_channel


def _plain(parts):
    return re.sub(r"\033\[[0-9;]*m", "", " ".join(parts))


def _payload(pct=None, read=None, write=None, usage=True):
    cw = {}
    if pct is not None:
        cw["used_percentage"] = pct
    if usage and (read is not None or write is not None):
        cw["current_usage"] = {"cache_read_input_tokens": read or 0,
                               "cache_creation_input_tokens": write or 0}
    return {"context_window": cw} if cw else {}


class ContextGaugeCase(unittest.TestCase):
    def setUp(self):
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        isolate_state_dir(self)
        # The render path brands the last line, and `_brand` renders the channel chip.
        # This case runs against the real plane on purpose; the chip is not what it is
        # about, so it is pinned rather than inherited (#459).
        pin_update_channel(self)
        # `test_gauge_appears_in_the_rendered_summary` calls `statusline.render`, which
        # persists usage under `config.SESSIONS_DIR` — isolate it so the suite never
        # writes into this repo's own `.charter/sessions/`.
        import shutil, tempfile
        from pathlib import Path
        from charter import config
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-usage-"))
        self._orig = config.SESSIONS_DIR
        config.SESSIONS_DIR = self.tmp / "sessions"
        self.addCleanup(lambda: (setattr(config, "SESSIONS_DIR", self._orig),
                                 shutil.rmtree(self.tmp, ignore_errors=True)))
        # Never fork a network child from the suite. This case renders against the REAL
        # plane on purpose, so `_brand`'s background version check would be spawned
        # against it, and `tests._planeguard.RealPlaneSpawn` refuses that outright (#527).
        # In `setUp` rather than in the one case that first needed it: three of the cases
        # below call `render`, and the precaution had been remembered in exactly one.
        from charter import update
        self.enterContext(mock.patch.object(update, "maybe_spawn", lambda: None))

    def test_shows_context_percentage(self):
        self.assertIn("ctx 37%", _plain(statusline._context_gauge(_payload(pct=37.4))))

    def test_cache_hit_rate_is_read_over_total_input(self):
        # 48000 read / (48000 + 1200 written) ≈ 98%
        self.assertIn("cache 98%", _plain(statusline._context_gauge(_payload(read=48000, write=1200))))

    def test_churning_prefix_reads_low(self):
        # cache creation dominating = the expensive failure mode
        self.assertIn("cache 11%", _plain(statusline._context_gauge(_payload(read=5000, write=40000))))

    def test_silent_before_first_api_call(self):
        # current_usage is null at session start and right after /compact — show nothing
        # rather than a misleading 0%
        self.assertEqual(statusline._context_gauge({}), [])
        self.assertEqual(statusline._context_gauge({"context_window": {}}), [])

    def test_silent_when_usage_present_but_empty(self):
        self.assertEqual(statusline._context_gauge(
            {"context_window": {"current_usage": {}}}), [])

    def test_full_cache_hit_is_100(self):
        self.assertIn("cache 100%", _plain(statusline._context_gauge(_payload(read=10000, write=0))))

    def test_cold_cache_is_zero_not_a_crash(self):
        self.assertIn("cache 0%", _plain(statusline._context_gauge(_payload(read=0, write=10000))))

    def test_context_and_cache_are_independent(self):
        # a payload with only cache data still renders the cache part
        out = _plain(statusline._context_gauge(_payload(read=100, write=100)))
        self.assertIn("cache 50%", out)
        self.assertNotIn("ctx", out)

    def test_gauge_appears_on_the_session_strip(self):
        """The gauges describe the SESSION, so they belong on the bottom strip with the
        rest of it — not in the top line, which answers only 'where am I'. Mixing them
        into that line was most of why the old header read as unrelated items."""
        out = statusline.render({"session_id": "t", **_payload(pct=42, read=900, write=100)})
        lines = [re.sub(r"\033\[[0-9;]*m", "", l) for l in out.splitlines() if l.strip()]
        # The status line is framed; the first and last rows are the box's own rules,
        # the zone dividers join its sides with tees, and every other row is bounded by
        # the frame's own verticals.
        lines = [l.strip("│ ") for l in lines if set(l) - set("┌─┐└┘├┤")]
        self.assertIn("ctx 42%", lines[-1])
        self.assertIn("cache 90%", lines[-1])
        self.assertNotIn("ctx", lines[0])

    def test_the_cache_gauge_is_labelled_not_a_glyph(self):
        """`⚡` used to mean "prompt-cache hit rate" here — a meaning nobody could guess
        from the character, on a strip where its labelled sibling `ctx NN%` sits two
        items away. The word was always the consistent choice; the bolt became actively
        wrong once the persona chips gave it a meaning of their own."""
        out = _plain(statusline._context_gauge(_payload(pct=42, read=900, write=100)))
        self.assertIn("cache 90%", out)
        self.assertNotIn("⚡", out)

    def test_the_bolt_on_the_strip_means_a_dispatch_and_nothing_else(self):
        """The assertion that keeps the rename from silently regressing.

        `⚡` is one fact — *a dispatch is running* — rendered in two places: on the
        persona chip that owns it, and as the aggregate here, which is what survives
        when the chip column is cropped away. Two of them on one line, told apart only
        by a `%`, and neither reads as anything.
        """
        from charter import inflight
        inflight.start("coder")
        out = statusline.render({"session_id": "t", **_payload(pct=42, read=900, write=100)})
        lines = [re.sub(r"\033\[[0-9;]*m", "", l) for l in out.splitlines() if l.strip()]
        strip = [l.strip("│ ") for l in lines if set(l) - set("┌─┐└┘├┤")][-1]
        self.assertIn("ctx 42%", strip)
        self.assertIn("cache 90%", strip)
        self.assertIn("⚡ 1", strip)
        self.assertEqual(strip.count("⚡"), 1, strip)

    def test_render_survives_a_malformed_payload(self):
        # the status line must never crash the footer
        for bad in ({"context_window": None}, {"context_window": {"current_usage": None}},
                    {"context_window": {"used_percentage": "nope"}}):
            self.assertIsInstance(statusline._context_gauge(bad), list)


class CacheTrendHintCase(unittest.TestCase):
    """The hint must stay SILENT normally and speak only with evidence — a sustained cold
    streak. One cold turn is normal (model switch, /compact, session warm-up)."""

    def setUp(self):
        isolate_state_dir(self)
        import shutil, tempfile
        from pathlib import Path
        from charter import config
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-usage-"))
        self._orig = config.SESSIONS_DIR
        config.SESSIONS_DIR = self.tmp / "sessions"
        self.addCleanup(lambda: (setattr(config, "SESSIONS_DIR", self._orig),
                                 shutil.rmtree(self.tmp, ignore_errors=True)))

    def _turn(self, read, write, sid="s"):
        return _plain(statusline._context_gauge(
            {"session_id": sid,
             "context_window": {"current_usage": {"cache_read_input_tokens": read,
                                                  "cache_creation_input_tokens": write}}}))

    def test_silent_below_the_streak_threshold(self):
        for i in range(statusline._COLD_STREAK - 1):
            out = self._turn(1000 + i, 90000 + i * 10)
            self.assertNotIn("cache cold", out)

    def test_fires_once_the_streak_is_reached(self):
        out = ""
        for i in range(statusline._COLD_STREAK):
            out = self._turn(1000 + i, 90000 + i * 10)
        self.assertIn("cache cold", out)
        self.assertIn("/rewind", out)      # names the cheaper remedy

    def test_goes_quiet_the_moment_the_cache_recovers(self):
        for i in range(statusline._COLD_STREAK + 2):
            self._turn(1000 + i, 90000 + i * 10)
        self.assertNotIn("cache cold", self._turn(95000, 500))

    def test_warm_session_never_nags(self):
        for i in range(10):
            self.assertNotIn("cache cold", self._turn(48000 + i * 100, 900))

    def test_rerenders_of_one_turn_do_not_inflate_the_streak(self):
        # the status line can render several times per turn; identical API numbers = same turn
        for _ in range(statusline._COLD_STREAK + 3):
            out = self._turn(1000, 90000, sid="dup")
        self.assertNotIn("cache cold", out)

    def test_trend_is_bounded(self):
        for i in range(statusline._TREND_KEEP * 3):
            self._turn(1000 + i, 90000 + i)
        rows = statusline._usage_file("s").read_text().splitlines()
        self.assertLessEqual(len(rows), statusline._TREND_KEEP)

    def test_streak_counts_only_the_tail(self):
        self.assertEqual(statusline._cold_streak([10, 90, 10, 20, 30]), 3)
        self.assertEqual(statusline._cold_streak([10, 20, 95]), 0)
        self.assertEqual(statusline._cold_streak([]), 0)

    def test_no_session_id_records_nothing(self):
        self.assertEqual(statusline._record_turn("", 10, 1, 9), [])


class RebuildDetectionCase(unittest.TestCase):
    """A cache REBUILD is the dominant cost and is invisible in the hit ratio — in steady
    state the ratio sits at ~100% because only the new exchange is written. Measured on a real
    session: one rebuild cost 696,088 tokens, ~139× what prompt trimming saves in a session.
    Signature: the read COLLAPSES (prefix no longer matched) while a large write replaces it."""

    def test_real_session_rebuild_is_detected(self):
        # verbatim shape of the observed event: steady ~712k reads, then read collapses
        rows = [(711912, 1622), (18954, 696088)]
        n, cost = statusline._rebuilds(rows)
        self.assertEqual(n, 1)
        self.assertEqual(cost, 696088)

    def test_steady_session_has_no_rebuilds(self):
        rows = [(700000 + i * 1000, 1200 + i) for i in range(12)]
        self.assertEqual(statusline._rebuilds(rows), (0, 0))

    def test_large_file_read_is_not_a_rebuild(self):
        # a big read appends a lot to cache, but the prefix still matched → read stays high
        self.assertEqual(statusline._rebuilds([(500000, 1000), (505000, 60000)])[0], 0)

    def test_multiple_rebuilds_accumulate(self):
        rows = [(700000, 1500), (10000, 500000), (60000, 1200), (5000, 300000)]
        n, cost = statusline._rebuilds(rows)
        self.assertEqual(n, 2)
        self.assertEqual(cost, 800000)

    def test_small_writes_never_count(self):
        self.assertEqual(statusline._rebuilds([(700000, 1000), (5, 900)]), (0, 0))

    def test_token_formatting(self):
        self.assertEqual(statusline._fmt_tok(696088), "696k")
        self.assertEqual(statusline._fmt_tok(1_400_000), "1.4M")
        self.assertEqual(statusline._fmt_tok(940), "940")

    def test_gauge_renders_rebuild_badge_and_explanation(self):
        import shutil, tempfile
        from pathlib import Path
        from charter import config
        tmp = Path(tempfile.mkdtemp(prefix="edm-rb-")) / "s"
        tmp.mkdir(parents=True)
        orig, config.SESSIONS_DIR = config.SESSIONS_DIR, tmp
        self.addCleanup(lambda: (setattr(config, "SESSIONS_DIR", orig),
                                 shutil.rmtree(tmp.parent, ignore_errors=True)))
        (tmp / "s1.usage").write_text("711912,1622,100\n18954,696088,3\n")
        out = _plain(statusline._context_gauge(
            {"session_id": "s1",
             "context_window": {"current_usage": {"cache_read_input_tokens": 730000,
                                                  "cache_creation_input_tokens": 1400}}}))
        self.assertIn("↻1 696k", out)          # cost stays on screen after the event
        self.assertIn("model/effort switch", out)  # and says what causes it


class RepoRowSigilCase(unittest.TestCase):
    """The change/MR cell renders each clone's OWN forge's notation — `!42` for a
    GitLab MR, `#42` for a GitHub PR — never a hardcoded `!`. An entry from a cache
    written by an older charter (no `sigil` at all, already normalised by
    `glstate.read_for` to `sigil: ""`) must still render, falling back to `!`."""

    def _row(self, gl_entry: dict) -> str:
        d = Path("/tmp/repo-x")
        rows = statusline._repo_rows([d], "ws", None, {}, {d: "main"}, {d: gl_entry})
        self.assertEqual(len(rows), 1, "one clone, no worktrees -> exactly one row")
        return statusline.tui.strip_ansi(rows[0].render(200)[0])

    def test_gitlab_backed_repo_renders_bang(self):
        self.assertIn("!42", self._row({"change": 42, "ci": None, "sigil": "!"}))

    def test_github_backed_repo_renders_hash(self):
        self.assertIn("#42", self._row({"change": 42, "ci": None, "sigil": "#"}))

    def test_old_shape_cache_entry_falls_back_to_bang(self):
        # what `glstate.read_for` hands back for an entry written before this upgrade:
        # `change` recovered from the old `mr` key, `sigil` absent -> "".
        self.assertIn("!7", self._row({"change": 7, "ci": None, "sigil": ""}))

    def test_no_open_change_renders_no_cell(self):
        line = self._row({"change": None, "ci": None, "sigil": ""})
        self.assertNotIn("!", line)
        self.assertNotIn("#", line)


class VaultHealthIsCachedOffTheRenderPath(PersonaIso):
    """A `1password` or `reference` vault's `health()` shells out to `op`, and both call
    sites ran it once per persona chip with no cache — on a status line that renders every
    turn. Profiled at 96% of render time; with a ~250ms `op` round trip a ten-persona
    roster measured ~3s of wall clock per turn, and with 1Password's desktop integration
    each call is a chance to raise an unprompted biometric prompt. The payoff on screen is
    one character."""

    def setUp(self) -> None:
        super().setUp()
        isolate_state_dir(self)
        statusline._vault_memo.clear()
        self.addCleanup(statusline._vault_memo.clear)
        from charter.secrets import registry
        registry.add_vault("v1", "plain-file", {"file": "a.json"})
        registry.add_vault("v2", "plain-file", {"file": "b.json"})
        self.calls = []

        class _Prov:
            id = "fake"
            def __init__(s, name): s.name = name
            def health(s):
                self.calls.append(s.name)
                return True, "2 secret(s)"

        # NOT `self._orig` — PersonaIso uses that name for its config snapshot.
        real_provider_for = registry.provider_for
        registry.provider_for = lambda n, doc=None: _Prov(n)
        self.addCleanup(lambda: setattr(registry, "provider_for", real_provider_for))

    def test_repeated_reads_of_one_vault_cost_one_check(self):
        for _ in range(5):
            statusline._vault_health("v1")
        self.assertEqual(self.calls, ["v1"])

    def test_a_second_render_reads_the_cache_from_disk(self):
        """The memo dies with the process — the status line is a fresh process per turn,
        so the disk TTL is the half that actually removes the per-turn cost."""
        statusline._vault_health("v1")
        statusline._vault_memo.clear()          # simulate the next render's process
        statusline._vault_health("v1")
        self.assertEqual(self.calls, ["v1"])

    def test_distinct_vaults_are_cached_separately(self):
        statusline._vault_health("v1")
        statusline._vault_health("v2")
        statusline._vault_health("v1")
        self.assertEqual(self.calls, ["v1", "v2"])

    def test_a_stale_entry_is_re_checked(self):
        import json as _json, time as _time
        statusline._vault_health("v1")
        statusline._vault_memo.clear()
        f = config.STATE_DIR / "cache" / "vaulthealth.json"
        doc = _json.loads(f.read_text())
        doc["v1"]["ts"] = _time.time() - statusline._VAULT_TTL - 1
        f.write_text(_json.dumps(doc))
        statusline._vault_health("v1")
        self.assertEqual(self.calls, ["v1", "v1"])


class TheRecordedRowCarriesTheContextPercentage(unittest.TestCase):
    """#413. `ctx NN%` is the one figure in the gauge that cannot be re-derived from
    anything: the cache ratio and the rebuild history both come out of `read`/`write`,
    and the percentage lives only in Claude Code's per-turn payload. Inside a frame the
    status line draws nothing and a panel draws instead, out of this file — so a
    percentage that was never written down is one the frame can never show.
    """

    def setUp(self):
        isolate_state_dir(self)
        import shutil
        import tempfile
        d = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, d, True)
        self._orig_sessions = config.SESSIONS_DIR
        config.SESSIONS_DIR = Path(d)
        self.addCleanup(lambda: setattr(config, "SESSIONS_DIR", self._orig_sessions))

    def _rows(self, sid="s1"):
        return [ln for ln in statusline._usage_file(sid).read_text().splitlines()
                if ln.strip()]

    def test_the_percentage_is_written_as_a_fourth_field(self):
        statusline.record_usage(dict(_payload(pct=42, read=900, write=100),
                                     session_id="s1"))
        self.assertEqual(self._rows(), ["900,100,90,42"])

    def test_a_turn_with_no_percentage_writes_an_empty_field_not_a_zero(self):
        """Early in a session, and right after `/compact`. `ctx 0%` would be a claim; an
        empty field is the absence of one, and `_last_ctx` skips it."""
        statusline.record_usage(dict(_payload(read=900, write=100), session_id="s1"))
        self.assertEqual(self._rows(), ["900,100,90,"])
        self.assertIsNone(statusline._last_ctx("s1"))

    def test_a_rerender_of_one_turn_is_still_one_row(self):
        """The de-duplication rule this file has always claimed — an identical
        `(read, write)` pair is the same API response re-rendered — asserted against a
        payload whose PERCENTAGE moved. Comparing the whole assembled row (what the code
        did while every field was derived from those two) would append a second row for
        the same turn, spending a slot of the ring buffer and shifting the rebuild
        history by one."""
        statusline.record_usage(dict(_payload(pct=42, read=900, write=100),
                                     session_id="s1"))
        statusline.record_usage(dict(_payload(pct=43, read=900, write=100),
                                     session_id="s1"))
        self.assertEqual(len(self._rows()), 1)

    def test_a_real_new_turn_still_appends(self):
        """The other direction, so the test above cannot be satisfied by a de-duplication
        that never appends anything."""
        statusline.record_usage(dict(_payload(pct=42, read=900, write=100),
                                     session_id="s1"))
        statusline.record_usage(dict(_payload(pct=44, read=1800, write=100),
                                     session_id="s1"))
        self.assertEqual(len(self._rows()), 2)

    def test_the_hit_percentage_is_read_positionally_not_as_the_last_field(self):
        """The trap the fourth field sets. `_hits` used to take the last field, which was
        the hit rate; it is now the CONTEXT percentage — a plausible number in the same
        0-100 range, so a trend reading it would have gone on rendering and quietly meant
        something else entirely. The fixture makes the two differ so the mistake cannot
        hide behind a coincidence."""
        statusline._record_turn("s1", 90, 900, 100, 42)
        self.assertEqual(statusline._hits(self._rows()), [90])

    def test_a_three_field_row_from_an_older_charter_still_reads(self):
        """The tracker keeps a session's history for its whole life, so an upgrade
        mid-session is ordinary. A row written before #413 has no fourth field: its hit
        rate and its `(read, write)` pair must both still be readable, or the rebuild
        counter would silently report "no rebuilds" where it means "charter stopped
        reading its own file"."""
        f = statusline._usage_file("s1")
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text("900,100,90\n1000,90000,1\n")
        self.assertEqual(statusline._hits(self._rows()), [90, 1])
        self.assertEqual(statusline._history("s1"), [(900, 100), (1000, 90000)])
        self.assertIsNone(statusline._last_ctx("s1"))

    def test_the_last_recorded_percentage_wins_over_an_empty_later_one(self):
        f = statusline._usage_file("s1")
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text("800,200,80,37\n900,100,90,\n")
        self.assertEqual(statusline._last_ctx("s1"), 37)


class ThePanelsGaugeReadsTheSameAsTheFooters(unittest.TestCase):
    """#413's own correctness condition. `top` inside a frame and Claude Code's footer
    outside one draw the same two numbers from two different sources — a live payload and
    a recorded history. If the thresholds or the labels drifted apart, a green 60% on one
    surface beside a yellow 60% on the other would be undebuggable from what is on screen.
    Both go through `_ctx_part`/`_cache_part`, and these tests are what keeps that true.
    """

    def setUp(self):
        isolate_state_dir(self)
        import shutil
        import tempfile
        d = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, d, True)
        self._orig_sessions = config.SESSIONS_DIR
        config.SESSIONS_DIR = Path(d)
        self.addCleanup(lambda: setattr(config, "SESSIONS_DIR", self._orig_sessions))

    def test_the_context_thresholds_are_the_ones_the_colours_are_chosen_from(self):
        """Agreement between the two surfaces (below) cannot see a threshold that moved on
        BOTH of them, since both go through one helper — so the thresholds themselves are
        pinned here, at the boundaries rather than in the middle of each band. The colour
        IS the fact: it is what turns a number into "this is fine" or "start a new
        session", and it is the half of the gauge nobody reads consciously."""
        cases = [(0, statusline._GREEN), (49, statusline._GREEN),
                 (50, statusline._YELLOW), (79, statusline._YELLOW),
                 (80, statusline._RED), (100, statusline._RED)]
        for pct, want in cases:
            with self.subTest(pct=pct):
                self.assertIn(want, statusline._ctx_part(pct))

    def test_the_cache_thresholds_are_pinned_the_same_way(self):
        """<50% sustained means the prefix is churning, which is the expensive failure
        mode this gauge was built to surface at all."""
        cases = [(0, statusline._RED), (49, statusline._RED),
                 (50, statusline._YELLOW), (79, statusline._YELLOW),
                 (80, statusline._GREEN), (100, statusline._GREEN)]
        for hit, want in cases:
            with self.subTest(hit=hit):
                self.assertIn(want, statusline._cache_part(hit))

    def test_the_two_surfaces_produce_byte_identical_markup(self):
        """Compared with the ANSI kept, not stripped: the colour IS the fact here — the
        thresholds are what turn a number into "this is fine" or "this is expensive"."""
        payload = dict(_payload(pct=85, read=400, write=600), session_id="s1")
        live = statusline._context_gauge(payload)[:2]
        panel = statusline.recorded_context_gauge("s1")[:2]
        self.assertEqual(panel, live)

    def test_the_rebuild_badge_reaches_the_panel_too(self):
        """`↻N <tokens>` is the number the ratio cannot show — one measured rebuild cost
        696k tokens and reads as a single dipped turn. It is computed from the recorded
        history on both surfaces, so a panel has no excuse for missing it."""
        f = statusline._usage_file("s1")
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text("900000,1000,99,40\n1000,600000,0,55\n")
        out = _plain(statusline.recorded_context_gauge("s1"))
        self.assertIn("↻1", out)

    def test_an_unknown_session_draws_nothing_rather_than_zeros(self):
        for sid in ("", "never-seen"):
            with self.subTest(sid=sid):
                self.assertEqual(statusline.recorded_context_gauge(sid), [])

    def test_an_unreadable_history_draws_nothing_rather_than_raising(self):
        """This is composed by a PANEL, where an exception is a hole in the frame rather
        than a traceback anybody sees."""
        with mock.patch("charter.statusline._usage_file",
                        side_effect=RuntimeError("boom")):
            self.assertEqual(statusline.recorded_context_gauge("s1"), [])


if __name__ == "__main__":
    unittest.main()
