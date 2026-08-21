# Proving a news-probe fix needs an OLD-CODE counterfactual, and 'git work

_2026-08-21 13:21 · persistent_

Proving a news-probe fix needs an OLD-CODE counterfactual, and 'git worktree add <scratch> v<prev>' is the clean way: a real checkout, so news._is_checkout passes and released() is non-empty (a hand-built scratch tree silently reports zero entries and every probe 'passes'). Assert the planted entry is in released() on BOTH sides before comparing. Use a vault name that does not exist so no credential is ever resolved — the status difference (old 'pending' = dispatched and exit code read, new 'unknown' = refused at the gate) is the evidence, no real vault needed.
