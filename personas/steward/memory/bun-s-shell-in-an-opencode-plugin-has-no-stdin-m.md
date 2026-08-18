# Bun's shell ($ in an opencode plugin) has NO .stdin() method — stdin goe

_2026-08-17 18:18 · persistent_

Bun's shell ($ in an opencode plugin) has NO .stdin() method — stdin goes in by redirection: $`cmd < ${new Blob([json])}` (Blob, Buffer and Response all work). Calling .stdin() throws TypeError, and if the plugin catches to fail open, the hook silently does nothing while doctor reports it wired. Iterating a plugin's JS costs ZERO model tokens: put the call in the plugin FACTORY, which runs on 'POST /session' against 'opencode serve', and log to a file — no turn needed. Only tool.execute.before/after need a real turn. Also: asserting generated code PARSES is not asserting it WORKS — the fix's regression test drives the hook with a $ stub that implements only .env/.quiet/.nothrow, no .stdin.
