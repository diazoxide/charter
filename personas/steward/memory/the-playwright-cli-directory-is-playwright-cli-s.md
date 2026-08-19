# The '.playwright-cli/' directory is @playwright/cli's OUTPUT dir (coreBu

_2026-08-19 15:43 · persistent_

The '.playwright-cli/' directory is @playwright/cli's OUTPUT dir (coreBundle.js: cliOutputDir, traceDir=.playwright-cli/trace) — traces, snapshots, screenshots. NOT session state: sessions live in ~/Library/Caches/ms-playwright/daemon/<hash>/<name>.session. '.playwright/' is a separate dir the installer creates for cli.config.json. This matters because charter issue #278 asserted the wrong one and the fix was nearly written against the report's description instead of the vendor's source. The real credential risk is traces: a trace records network requests WITH headers and bodies, so tracing a 'charter secret exec' login writes to disk the password the bridge kept out of the transcript. Upstream precedent worth citing: create-playwright's _patchGitIgnore() already gitignores /playwright/.auth/ and /test-results/ append-only+idempotently. Filed microsoft/playwright#42307.
