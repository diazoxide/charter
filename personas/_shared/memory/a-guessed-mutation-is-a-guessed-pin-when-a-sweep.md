# A guessed mutation is a guessed pin. When a sweep names a survivor, appl

_2026-09-12 13:00 · persistent_

A guessed mutation is a guessed pin. When a sweep names a survivor, apply the mutation VERBATIM from its artifact (gh run download → sweep-results-*.json has before/after/line), never retyped from the warning text. Measured 2026-09-12 on charter #989: three hand-written mutations all died locally and contradicted CI — the easy read was 'CI is flaky'. CI's real ones survived, and that exposed two guards pinned by macOS's /var → /private/var symlink that would have been unpinned on every Linux run. The tell was in the record: a survivor whose 'full' field reads 'red once … green on confirmation' is platform-dependent, not flaky. A fixture that needs a symlink must CREATE one (patch config.WORKSPACES_DIR at a link it makes) rather than lean on the platform's temp dir.
