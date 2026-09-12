---
version: unreleased
headline: A chat starts on the harness profile you name — its own command and config folder, never through tmux
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

`charter claude-work` runs that profile's command with its env. A profile whose command or
environment is new or has changed since it last ran shows it and asks `run this? [y/N]`
once; a reopen, a handoff, or a launch with no terminal to ask in refuses it instead.

```
charter: profile 'claude-work' is new — it has not run on this machine before.
  command  claude
  env      CLAUDE_CONFIG_DIR=~/.claude-work
run this? [y/N]
```

A yes is written down and that profile starts without a word until one of those three
changes. The built-in profiles — `claude`, `codex`, `opencode` — never ask: their command
comes out of charter's own registry rather than out of a file. `charter <profile>` asks
before tmux; the `+` and a workspace tab ask in the new chat's own pane, because the press
behind them has no terminal and the pane has one. The record lives under `.charter/`, which
a chat can write as easily as it can write `charter.local.toml` — so this catches a command
you did not change yourself unless whatever changed it also forged the record.

**A profile is wired, or it refuses to start.** Charter's guard lives in the harness's own
config folder, and a profile names another one — so charter asks the harness, under that
profile's own command and environment, whether its plugin or shim is actually there. A
profile whose folder does not carry it refuses to launch, says what is missing and prints
`charter harness install <profile>`, which wires that folder. A probe that cannot answer
refuses too, and names the command to run by hand: an unknown is not a pass. `charter
doctor` now shows a row per profile.

**This includes the built-ins.** `charter codex` on a plane where nobody wired Codex, and
`charter opencode` where `init` never wrote its shim, now refuse where they used to start. A
chat that looks guarded and is not is the same failure whichever profile started it, and no
flag starts one unguarded.

`charter init` installs for each Claude Code and opencode profile you declared; `charter
reinit` installs nothing, writing only the opencode shim and naming the install command for
anything else; Codex stays opt-in through `charter harness install`, which prints the
plugin and hook-approval steps Codex only accepts from you, with `CODEX_HOME=` in front.

**Codex users: approve charter's SessionStart hook once more.** Its command gained
`--preflight`, and Codex trusts a hook by the hash of its command. The flag is what keeps
the probes off the hook path: `charter doctor --preflight` runs every other check and asks
no harness anything.
