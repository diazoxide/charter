---
version: unreleased
headline: `charter persona default --clear` prints `Cleared` only when the default is gone, and names the file it could not change instead of crashing
---

`charter persona default --clear` printed `✓ Cleared the declared default persona` and
exited 0 when `charter.toml` could not be written, for example when the file was read-only.
The declaration was still in the file, so every session with nothing else chosen went on
adopting that persona after the command said it would not.

When `personas/`, the directory holding the legacy `personas/.default`, was read-only, the
command ended in a Python traceback instead.

Now, when either file cannot be changed, it names the file and the system's error and exits
1. It prints `Cleared` only when both are gone, and `No default persona was declared.` when
neither was there, as before. A read-only `charter.toml` whose `[persona]` section has
nothing left to remove is no longer rewritten, so it does not fail a clear either.
