"""The provider's own tests.

pytest, not `unittest` — CONTRIBUTING.md's rule ("Tests for anything you add to charter
itself are stdlib unittest") is about charter's tree, and this package is not in it. That
asymmetry is itself part of what the experiment is testing: a provider is a separate
distribution with its own toolchain, and if it could not choose one the entry-point seam
would not be doing its job.

Nothing here starts a tmux server. Everything that needs a real terminal is in
`measure/`, run by hand, and reported with the numbers it produced.
"""

from __future__ import annotations

import time

import pytest

from charter.frame import ctx as charter_ctx
from charter.frame import registry
from charter.frame.component import Component

import charter_textual_repos as prov
from charter_textual_repos import rows


SNAPSHOT = {
    "gathered_at": time.time(),
    "workspace": "harness-wrapper",
    "current_repo": "charter",
    "repos": [
        {"name": "charter", "branch": "main", "dirty": True, "tracked_dirty": True,
         "ahead": 2, "behind": 0, "ci": "failed", "change": 554, "sigil": "!",
         "current": True, "worktree_count": 3},
        {"name": "infra", "branch": "main", "dirty": False, "tracked_dirty": False,
         "ahead": 0, "behind": 1, "ci": "passed", "change": None, "sigil": "",
         "current": False, "worktree_count": 0},
    ],
    "worktrees": [
        {"name": "pr-554", "branch": "pr-554", "dirty": True, "tracked_dirty": False,
         "ahead": 1, "behind": 0, "ci": None, "change": None, "sigil": "",
         "current": False, "worktree_count": 0},
    ],
    "todos": [], "todo_count": 0,
}


def build_ctx(width=90, height=10, snapshot=SNAPSHOT):
    return charter_ctx.build(prov.NEEDS, width=width, height=height, fid="t",
                             snapshot=snapshot)


class TestDiscovery:
    """Charter finds this package without charter changing — the whole premise."""

    def test_both_components_are_declared_in_the_entry_point_group(self):
        supplied = registry.Registry().providers.ids()
        assert "textual.repos" in supplied
        assert "textual.live" in supplied

    def test_placing_loads_the_component_rather_than_a_standin(self):
        reg = registry.Registry()
        component = reg.place("textual.repos")
        assert isinstance(component, Component)
        assert reg.failures == {}

    def test_the_entry_point_name_is_the_component_id(self):
        # `Providers._one` refuses a mismatch, because the entry point name is what a
        # committed charter.toml places and what charter resolves without importing.
        assert prov.adapter_component().id == "textual.repos"
        assert prov.live_component().id == "textual.live"

    def test_the_api_version_is_the_one_charter_speaks(self):
        from charter.frame.component import API_VERSION
        assert prov.API_VERSION == API_VERSION


class TestRows:
    """The data reduction, which is where a column is right or wrong."""

    def test_every_column_comes_from_the_gather_snapshot(self):
        got = rows.rows_of(SNAPSHOT)
        assert [r.name for r in got] == ["charter ⑂3", "infra", "pr-554"]
        # The piece's branch is blank on purpose: `slots._table_lines` passes
        # `branch_override=""` for a piece whose branch merely restates its own name, and
        # a column repeating the cell beside it is a column spent saying nothing.
        assert [r.branch for r in got] == ["main", "main", ""]
        assert [r.marks for r in got] == ["* ↑2", "↓1", "? ↑1"]
        assert [r.ci for r in got] == ["fail", "ok", ""]
        assert [r.change for r in got] == ["!554", "", ""]

    def test_a_tracked_dirty_tree_is_told_apart_from_an_untracked_one(self):
        # charter's own `statusline._markers` makes this distinction and the frame's table
        # shows it; a provider that flattened both to "dirty" would be quietly lying about
        # whether there is anything to commit.
        assert rows.markers({"dirty": True, "tracked_dirty": True}) == "*"
        assert rows.markers({"dirty": True, "tracked_dirty": False}) == "?"
        assert rows.markers({"dirty": False}) == ""

    def test_a_cold_cache_draws_no_rows_rather_than_inventing_any(self):
        assert rows.rows_of({}) == []
        assert rows.rows_of(None) == []


@pytest.fixture(scope="module")
def reg():
    """A registry with the adapter placed, and its background app stopped afterwards.

    Module-scoped because the app is process-lifetime state by design
    (`adapter._HOST`), exactly as it is in a panel process: charter builds the registry
    once and closes over it, so a per-test app would be measuring a shape production
    never has.
    """
    reg = registry.Registry()
    reg.place("textual.repos")
    yield reg
    from charter_textual_repos import adapter
    adapter._HOST.stop()


class TestAdapterUnderCharter:
    """The adapter, driven through `Registry.draw` — charter's own call path."""

    def test_it_draws_the_repo_table_into_the_rectangle_it_was_given(self, reg):
        drew = reg.draw("textual.repos", build_ctx())
        assert len(drew) == 10
        assert all(len(line) <= 90 for line in drew)
        assert "repos 2" in drew[0]
        assert "charter" in drew[2]

    def test_a_later_snapshot_reaches_the_widget(self, reg):
        later = dict(SNAPSHOT, repos=[dict(SNAPSHOT["repos"][0], ci="passed")],
                     worktrees=[])
        drew = reg.draw("textual.repos", build_ctx(snapshot=later))
        assert any("ok" in line for line in drew)
        assert not any("fail" in line for line in drew)

    def test_the_lines_carry_no_escape_at_all(self, reg):
        # Not a style choice: `Registry.draw` escapes every line a provider returned
        # (`escape=cid in self._foreign`), so an ANSI-bearing line would be painted as the
        # literal characters `\x1b[38;2;...`. `Strip.text` is what makes that a non-issue,
        # at the cost of every colour Textual computed.
        drew = reg.draw("textual.repos", build_ctx())
        assert not any("\x1b" in line or "\\x1b" in line for line in drew)

    def test_a_pane_with_no_room_is_no_lines_rather_than_a_crash(self, reg):
        assert reg.draw("textual.repos", build_ctx(width=0, height=0)) == ()


class TestTheContractItselfHeld:
    """What the experiment set out to check, asserted rather than asserted about."""

    def test_ctx_serves_only_what_was_declared(self):
        c = build_ctx()
        assert c.gather["current_repo"] == "charter"
        with pytest.raises(AttributeError):
            c.todos                                   # not in NEEDS
        with pytest.raises(AttributeError):
            c.subprocess                              # never served at all

    def test_declaring_an_event_kind_charter_cannot_deliver_is_still_accepted(self):
        # `component.EVENT_KINDS` is validated at construction and read by nothing else in
        # charter. This test passes, and that it passes is finding #1: `textual.live`
        # declares four kinds, receives them from the tty itself, and charter neither
        # delivers them nor knows it is not delivering them.
        assert prov.live_component().events == ("key", "click", "scroll", "resize")

    def test_a_size_policy_other_than_fixed_survives_only_where_config_is_silent(self):
        # `instance.component_tables` coerces a provider's committed `size` to `Fixed(n)`,
        # so `Content(cap=…)` and `Fill()` are reachable only on a frame nobody arranged.
        from charter.frame.component import Fixed
        assert isinstance(prov.adapter_component().size, Fixed)
