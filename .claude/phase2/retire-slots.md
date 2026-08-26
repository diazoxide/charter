# retire-slots
PR: https://github.com/diazoxide/charter/pull/553 — open, NOT merged
Branch: worktree-wf_ce811d23-e06-1 (commit 04c545e, pushed)

## Blocking

### 1

charter/instance.py:841-843 — the provider branch's `edge`/`size` value validation has NO test, and three separate deletions of it keep all 5906 tests green (each confirmed against the full suite, not a subset). (a) Dropping `isinstance(size, bool) or size < 1`: full suite OK. (b) Replacing the size checks with `size is None`: full suite OK. (c) Replacing `edge not in EDGES` with `edge is None`: full suite OK. The consequence of (a)/(b) is not cosmetic — I ran it end to end: a committed `[[frame.component]]` table with `size = 0` (also `true`, `-4`, `"12"`) makes `Fixed.__post_init__` raise `ComponentError` out of `component_tables` → `frame_of` → `config.derive`, which sits OUTSIDE the try/except that catches a malformed charter.toml, so `import charter.config` dies and EVERY charter command including `charter --version` crashes on that clone. The shipped code refuses all six values correctly and returns `None`; the point is that nothing pins it. (c) lets an unvalidated edge string (`"sideways"`, `""`) be placed and travel into `layout._edge_of`, where it falls out of `_COLUMN_EDGES`/`_ROW_EDGES`/`_BEFORE_EDGES` and silently becomes a plain `-v` after-split. The existing `test_a_provider_placed_without_a_rectangle_refuses_the_arrangement` (tests/test_component_id_is_the_currency.py:260) covers only an ABSENT key, never a present-but-unusable value. Fix: extend that subTest loop with `{"use": CID, "edge": "right", "size": 0}`, `size=True`, `size=-4`, `size="12"`, `edge="sideways"`, `edge=""` — each must answer `None` — and re-run all three mutations to RED. This is the plan's own global constraint ("Mutation-test every guard") applied to the one guard on the new config boundary.

