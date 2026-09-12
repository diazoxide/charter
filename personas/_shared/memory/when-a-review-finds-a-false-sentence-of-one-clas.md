# When a review finds a false sentence of one CLASS (for example a success

_2026-09-10 18:54 · persistent_

When a review finds a false sentence of one CLASS (for example a success message claiming a lock charter does not hold), make round 1's fix brief demand a grep sweep of every site that prints that class, not just the sites the reviewer named. On PR 939 on 2026-09-10 the same false lock claim was found at use, then create --use, then rename, across three review rounds, because each round fixed only the named sites. Also: a grep for DIRECT callers is not a reachability check. A reviewer wrongly concluded that opencode's context file lacks the workspace-confirm nudge, because context_block calls it only through _context_parts.
