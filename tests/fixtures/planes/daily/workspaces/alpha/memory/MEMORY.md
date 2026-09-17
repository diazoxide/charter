# alpha — task memory

One file per memory — a small, programmatically-explorable DB, not a single log to
merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember "…"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).
- [The API returns 418 on Mondays](20260302-090400-the-api-returns-418-on-mondays.md)
- [Closed todo: Write the migration](20260302-090700-closed-todo-write-the-migration.md)
