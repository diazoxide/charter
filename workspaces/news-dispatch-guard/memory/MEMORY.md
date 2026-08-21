# news-dispatch-guard — task memory

One file per memory — a small, programmatically-explorable DB, not a single log to
merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember "…"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).
- [Closed todo: Fix #311 — re-entrancy guard in news._dispatch (branch news](20260820-232340-closed-todo-fix-311-re-entrancy-guard-in-news-di.md)
- [#311 delivered — PR #313 merged as c7a665a, 2876 tests OK, unreleased (r](20260820-232410-311-delivered-pr-313-merged-as-c7a665a-2876-test.md)
- [0.47.1 cut and tagged (2026-08-21): patch carrying #313 only — the news ](20260821-104805-0-47-1-cut-and-tagged-2026-08-21-patch-carrying-.md)
- [Closed todo: Close #314 — cross-process probe guard (branch probe-guard-](20260821-122126-closed-todo-close-314-cross-process-probe-guard-.md)
- [Closed todo: Close #317 — a check: can reach arbitrary argv via 'secret ](20260821-125139-closed-todo-close-317-a-check-can-reach-arbitrar.md)
- [0.47.2 shipped (2026-08-21): #318 cross-process probe guard + #319 check](20260821-133144-0-47-2-shipped-2026-08-21-318-cross-process-prob.md)
