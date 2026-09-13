---
version: unreleased
headline: `charter harness list` shows a profile's `~/…` program the way a shell would need it typed
---

A profile whose command starts with `~` — `command = ["~/.local/bin/codex"]` — listed as
`'~/.local/bin/codex'`. A shell does not expand a quoted `~`, so that line, pasted into a
terminal, looked for a directory literally named `~`. Charter expands the program's leading
`~` itself, so `charter harness list`, `charter doctor` and the profile selector now show it
bare and quote only what follows: `~/.local/bin/codex`, or `~/'my tools/codex'`. A `~` in
any later word is not expanded — it reaches the harness as written — so it stays quoted.
