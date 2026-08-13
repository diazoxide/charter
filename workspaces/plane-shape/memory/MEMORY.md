# plane-shape — task memory

One file per memory — a small, programmatically-explorable DB, not a single log to
merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember "…"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).
- [Tickets 01, 03, 04 merged (#62, #63, #65); 02 in review as #64. Correcti](20260812-144109-tickets-01-03-04-merged-62-63-65-02-in-review-as.md)
- [Decision: discover should be optional. On a personal account it enumerat](20260812-150638-decision-discover-should-be-optional-on-a-person.md)
- [An error message may classify a failure it recognised; it must never ass](20260813-120419-an-error-message-may-classify-a-failure-it-recog.md)
