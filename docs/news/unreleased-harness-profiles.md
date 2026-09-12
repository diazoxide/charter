---
version: unreleased
headline: A new chat asks which harness profile to start — no harness runs until you pick one
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

**And no harness starts until you pick a profile.** Before any harness has run in it, a
chat's pane draws the **profile selector** — the `F2` palette's own picker, so you type to
narrow it and press Enter to start. Every profile you declared is a row; a built-in is a row
where its program is installed; a profile that cannot start is listed with the reason on it,
and Enter there shows the reason and leaves the list open rather than closing the chat. A new
or changed profile shows its command and asks in place. Esc closes that chat having started
nothing — a pane at the selector has a tab, but it is not a chat: `charter: quit` does not
record it and `charter reopen` never brings it back. It always shows, even where one profile
can run: one profile costs one Enter, and skipping it would bring back the harness nobody
picked.

**Where it appears:** bare `charter` on a workspace with no running chat, the `+` on the
chat strip, the palette's `chat: new`, and a workspace tab whose workspace has nothing
running. **Where it does not:** `charter <profile>` names the profile, and so does every
open nobody is at — `charter reopen`, a restored plane, and a chat handed off by another
chat. A selector is a question, and those have nobody there to answer one.

Two stops went with it. **`[harness] default` no longer launches anything** — it chooses
which row the cursor starts on, and a value naming a profile this machine does not have
marks no row instead of refusing the command; `charter doctor` is where that typo is now
reported. And the `+` no longer refuses a chat whose profile the plane no longer declares,
or a plane that declares no default: it opens the selector, which says on each row why
anything cannot start. Bare `charter` on a plane that declares nothing opens it too, rather
than printing the usage list.

**Bare `charter` on a workspace that is already running attaches to it** whether or not
anybody else is, instead of adding a chat. It names nothing — it means *put me in this
plane* — and the workspace already has chats. `+` is how you add one, and `charter
<profile>` or `charter frame -- <cmd>` still opens a chat and runs what it names. Piped
anywhere, bare `charter` still prints its usage: a pipe is no place to draw a selector.

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

Nothing is asked of a profile before you have approved its command, because asking runs
it. Once you have, `charter init` installs for each Claude Code and opencode profile you
declared; `charter reinit` installs nothing, writing only the opencode shim and naming the
install command for anything else; Codex stays opt-in through `charter harness install`,
which asks first like a launch does and prints the plugin and hook-approval steps Codex
only accepts from you, with `CODEX_HOME=` in front.

**Codex users: approve charter's SessionStart hook once more.** Its command gained
`--preflight`, and Codex trusts a hook by the hash of its command. The flag is what keeps
the probes off the hook path: `charter doctor --preflight` runs every other check and asks
no harness anything. The CLI and the plugin release together, and until the CLI is updated
too, a session start with the new plugin runs **no preflight check at all**: the older CLI
rejects the flag, the hook still exits 0, and the session opens with `charter preflight
failed - fix before working:` over `charter: error: unrecognized arguments: --preflight`.
`charter update` moves the CLI, and the line goes away.
