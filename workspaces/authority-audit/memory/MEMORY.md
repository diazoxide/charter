# authority-audit — task memory

One file per memory — a small, programmatically-explorable DB, not a single log to
merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember "…"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).
- [Closed todo: Sweep 1: charter-file parsing → authority (dispatched to st](20260821-163702-closed-todo-sweep-1-charter-file-parsing-authori.md)
- [Closed todo: Sweep 2: forge data → authority (dispatched to forge, read-](20260821-163702-closed-todo-sweep-2-forge-data-authority-dispatc.md)
- [Forge-data security audit (surface 2): filed #323 (gh -F @-filename via ](20260821-175329-forge-data-security-audit-surface-2-filed-323-gh.md)
- [Closed todo: Fix #328 — containment helper at every parse boundary (bran](20260821-184431-closed-todo-fix-328-containment-helper-at-every-.md)
- [#328 + #325/#334/#335/#337 closed by PR #342 (6f385ed): new charter/cont](20260821-184432-328-325-334-335-337-closed-by-pr-342-6f385ed-new.md)
