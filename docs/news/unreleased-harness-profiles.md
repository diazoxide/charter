---
version: unreleased
headline: Charter reads harness profiles from charter.local.toml, a file it keeps out of git
adopt: reinit
---

Two Claude Code accounts in two config folders, or a Codex pinned to an older release, had no
way to reach charter. It ran one program per harness, one way, with no shell in between, so an
alias never resolved. A **harness profile** is that way: a kind, a command and an environment,
declared in `charter.local.toml` beside `charter.toml`.

```toml
[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.claude-work" }
```

`charter harness list` now shows every profile charter read — a built-in for each harness,
named after it, and yours — with the file each came from and why any was refused: a kind
charter cannot launch, a command written as a shell string, a name `charter` already uses, a
variable named like a credential. Charter holds no credential in a profile, because anything
set on the harness reaches the model's own shell; that refusal names the harness's own login
instead.

A profile's command will run on a click, so the file must never be committed. `charter init`
writes `/charter.local.toml` into a new plane's `.gitignore` and `charter reinit` adds it to
yours. While git tracks the file or would commit it, `charter harness list` shows every
profile in it refused and names the fix, and `charter doctor` warns on its `harness profiles`
row.

This release does not launch a profile yet. `charter claude`, bare `charter` and every chat
start exactly as they did.
