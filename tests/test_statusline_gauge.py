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

from charter import config, statusline
from tests._isolation import PersonaIso, isolate_state_dir


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
        isolate_state_dir(self)
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

    def test_shows_context_percentage(self):
        self.assertIn("ctx 37%", _plain(statusline._context_gauge(_payload(pct=37.4))))

    def test_cache_hit_rate_is_read_over_total_input(self):
        # 48000 read / (48000 + 1200 written) ≈ 98%
        self.assertIn("⚡98%", _plain(statusline._context_gauge(_payload(read=48000, write=1200))))

    def test_churning_prefix_reads_low(self):
        # cache creation dominating = the expensive failure mode
        self.assertIn("⚡11%", _plain(statusline._context_gauge(_payload(read=5000, write=40000))))

    def test_silent_before_first_api_call(self):
        # current_usage is null at session start and right after /compact — show nothing
        # rather than a misleading 0%
        self.assertEqual(statusline._context_gauge({}), [])
        self.assertEqual(statusline._context_gauge({"context_window": {}}), [])

    def test_silent_when_usage_present_but_empty(self):
        self.assertEqual(statusline._context_gauge(
            {"context_window": {"current_usage": {}}}), [])

    def test_full_cache_hit_is_100(self):
        self.assertIn("⚡100%", _plain(statusline._context_gauge(_payload(read=10000, write=0))))

    def test_cold_cache_is_zero_not_a_crash(self):
        self.assertIn("⚡0%", _plain(statusline._context_gauge(_payload(read=0, write=10000))))

    def test_context_and_cache_are_independent(self):
        # a payload with only cache data still renders the cache part
        out = _plain(statusline._context_gauge(_payload(read=100, write=100)))
        self.assertIn("⚡50%", out)
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
        self.assertIn("⚡90%", lines[-1])
        self.assertNotIn("ctx", lines[0])

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


if __name__ == "__main__":
    unittest.main()


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
