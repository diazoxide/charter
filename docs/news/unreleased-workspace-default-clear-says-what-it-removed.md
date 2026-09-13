---
version: unreleased
headline: `charter workspace default --clear` removes the nominated default without a name, and prints `Cleared` only when it did
---

`charter workspace default --clear` with no name printed the current default, exited 0 and
removed nothing. Clearing worked only with a name typed beside the flag, and any string
would do. It now checks `--clear` first, as `charter persona default` does, so a name
beside it is still accepted and not used.

It also printed `✓ Cleared the declared default workspace.` in two cases where it had
removed nothing: when no default was nominated, and when `workspaces/.default` could not be
removed, for example with `workspaces/` read-only. In the second case sessions with nothing
else selected went on landing on that workspace after the command said they would not.
Now it says `No default workspace was declared.` in the first case. In the second it names
the file and the system's error and exits 1.
