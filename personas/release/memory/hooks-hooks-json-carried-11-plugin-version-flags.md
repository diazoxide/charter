# hooks/hooks.json carried 11 --plugin-version flags at 0.47.2, not the 9

_2026-08-21 13:20 · persistent_

hooks/hooks.json carried 11 --plugin-version flags at 0.47.2, not the 9 the persona charter names — the count grows silently as hooks are added. Never work from a remembered count: sed the substitution globally, then re-grep the OLD string across all four files and count the NEW one (14 total at 0.47.2 = 1 pyproject + 1 __init__ + 1 plugin.json + 11 hooks).
