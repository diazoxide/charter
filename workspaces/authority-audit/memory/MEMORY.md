# authority-audit — task memory

One file per memory — a small, programmatically-explorable DB, not a single log to
merge-conflict on. Files are timestamp-prefixed, so this index (and the directory) list chronologically. **Committed + shared** for LIVE workspaces. Write with `charter workspace remember "…"`, search with `charter workspace recall [--query …]`, drop one with `charter workspace forget <slug>`. Never put secrets here (vault only).
- [Closed todo: Sweep 1: charter-file parsing → authority (dispatched to st](20260821-163702-closed-todo-sweep-1-charter-file-parsing-authori.md)
- [Closed todo: Sweep 2: forge data → authority (dispatched to forge, read-](20260821-163702-closed-todo-sweep-2-forge-data-authority-dispatc.md)
- [Forge-data security audit (surface 2): filed #323 (gh -F @-filename via ](20260821-175329-forge-data-security-audit-surface-2-filed-323-gh.md)
- [Closed todo: Fix #328 — containment helper at every parse boundary (bran](20260821-184431-closed-todo-fix-328-containment-helper-at-every-.md)
- [#328 + #325/#334/#335/#337 closed by PR #342 (6f385ed): new charter/cont](20260821-184432-328-325-334-335-337-closed-by-pr-342-6f385ed-new.md)
- [Closed todo: Wave 1: #330/#331/#332 (secrets surface, authority-audit cl](20260822-001231-closed-todo-wave-1-330-331-332-secrets-surface-a.md)
- [Wave 1 done: #324/#326 (PR 344) and #330/#331/#332 (PR 346) merged; 3043](20260822-001231-wave-1-done-324-326-pr-344-and-330-331-332-pr-34.md)
- [#336 boundary correction: .charter/ sits UNDER the plane ROOT, so 'refus](20260822-005353-336-boundary-correction-charter-sits-under-the-p.md)
- [Open finding, NOT closed by PR #348: a committed symlink redirects chart](20260822-005356-open-finding-not-closed-by-pr-348-a-committed-sy.md)
- [Write-side containment (#349/#350): contain.write_refusal is file_refusa](20260822-083033-write-side-containment-349-350-contain-write-ref.md)
- [Vacuous-pass traps found this round (authority audit): (1) a ONE-LINE ta](20260822-083046-vacuous-pass-traps-found-this-round-authority-au.md)
- [#343 ruling (won't-fix on revocation): after #342 a broken persona refer](20260822-083046-343-ruling-won-t-fix-on-revocation-after-342-a-b.md)
- [0.48.0 SHIPPED (2026-08-22): the authority audit and all its fixes — 23 ](20260822-125704-0-48-0-shipped-2026-08-22-the-authority-audit-an.md)
- [Closed todo: AFTER #328: #331 needs a non-mutating health() (doctor chmo](20260822-125729-closed-todo-after-328-331-needs-a-non-mutating-h.md)
- [Closed todo: Wave 2 after wave 1 merges: #336 symlinks+liveness · #333/#](20260822-125729-closed-todo-wave-2-after-wave-1-merges-336-symli.md)
- [Closed todo: Wave 3a: #349/#329/#343 (authority-audit) + #333/#338/#339 ](20260822-125729-closed-todo-wave-3a-349-329-343-authority-audit-.md)
- [Closed todo: Wave 3b after 3a: #321 news diagnostic + #322 secret list l](20260822-125729-closed-todo-wave-3b-after-3a-321-news-diagnostic.md)
