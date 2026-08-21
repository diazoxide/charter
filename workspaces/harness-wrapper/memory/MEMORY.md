# harness-wrapper — task memory

One file per memory — a small, programmatically-explorable DB, not a single log to
merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember "…"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).
- [Task 1 of harness-frame plan: added Harness.cli_name, Harness.binary, an](20260822-002345-task-1-of-harness-frame-plan-added-harness-cli-n.md)
