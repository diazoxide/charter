# Pre-existing leak-guard hole on charter main a5aa860 (measured 2026-09-1

_2026-09-11 12:31 · persistent_

Pre-existing leak-guard hole on charter main a5aa860 (measured 2026-09-11): hooks._leak_reason("cat x && bash <<'EOF'\ncat .charter/vaults/dev.json\nEOF") returns None, i.e. ALLOWED. _strip_reader_heredocs strips a heredoc body when _reader_of(line) is true, and _reader_of asks whether the line's FIRST program is a reader (cat), so a later segment's script body (bash's) is dropped before the leak check. The chat-handoff task 4 predicate for a handoff's brief (_feeds_a_handoff) avoids the same hole by requiring the heredoc to sit on the handoff's own segment (the text before the first '<<' must segment to exactly one handoff invocation); _reader_of itself was left unchanged and needs its own fix.
