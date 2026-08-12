# user-reporting — task memory

One file per memory — a small, programmatically-explorable DB, not a single log to
merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember "…"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).
- [Grilled the reporting design to an empty frontier (5 rounds). Settled: r](20260811-002821-grilled-the-reporting-design-to-an-empty-frontie.md)
- [Implemented the full spec on branch upstream-reporting (2 commits: a75b6](20260811-011026-implemented-the-full-spec-on-branch-upstream-rep.md)
