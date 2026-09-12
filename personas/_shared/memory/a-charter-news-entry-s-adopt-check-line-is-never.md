# A charter news entry's adopt:/check: line is never a shell string (chart

_2026-09-11 13:07 · persistent_

A charter news entry's adopt:/check: line is never a shell string (charter/news.py _tokens, measured 2026-09-11 on main a5aa860): it refuses any action holding a character in _SHELLISH (; | & < > $ backtick ( ) backslash newline, double and single quote) and splits the rest on whitespace, and tests/test_news.py test_every_shipped_action_resolves_against_the_live_parser fails an entry whose action does not parse. So a command whose argument needs quotes, e.g. guard ask 'charter handoff *', cannot be an adopt: line. Chat-handoff task 4 added charter guard handoff (delegates to cmd_guard_ask with the fixed pattern) for exactly that reason. skills/update/SKILL.md mentions adopt: manual, but news.py has no handling for it.
