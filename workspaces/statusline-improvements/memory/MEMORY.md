# statusline-improvements — task memory

One file per memory — a small, programmatically-explorable DB, not a single log to
merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember "…"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).
- [Grilled the three statusline asks with the operator (2026-08-20). Asks 1](20260820-125834-grilled-the-three-statusline-asks-with-the-opera.md)
