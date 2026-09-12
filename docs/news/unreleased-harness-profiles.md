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
instead. It is a refusal to hold one and not a barrier against one: the pattern can catch an
innocent name — `KEYBOARD_LAYOUT` contains `KEY` — and a wrapper script named in `command`
can export whatever it likes.

A profile's command will run on a click, so the file must never be committed. `charter init`
writes `/charter.local.toml` into a new plane's `.gitignore` and `charter reinit` adds it to
yours. While git tracks the file or would commit it, `charter harness list` shows every
profile in it refused and names the fix, and `charter doctor` warns on its `harness profiles`
row.

**Every chat now starts through charter's own launcher.** The first process in a chat's
pane is charter, which checks the profile and then replaces itself with the harness — so a
profile's environment reaches the harness without passing through tmux, and the pane, its
exit code and everything charter reads off it stay exactly what they were. The chat records
the profile it runs: the `+`, a workspace tab and a chat handed off by another chat all
open on that same profile rather than merely on the same kind of harness, because two chats
of one harness may be two accounts.

`charter reopen` brings each chat back on its own profile, and a chat whose profile is gone
is **skipped by name** rather than moved onto another one — another profile may be another
account, where that conversation does not exist. It stays in the record, so declaring the
profile again and running `charter reopen` brings it back.

This release still does not launch a profile you declared: it is refused by name, with a
sentence saying charter cannot yet ask before a declared command runs. Nothing a chat could
have written into `charter.local.toml` runs until the release that adds the asking. The
built-in profiles — `claude`, `codex`, `opencode` — start exactly as they did.
