# A handoff's brief runs with your authority, so your harness asks you first

A handoff opens a chat, in this workspace or another, whose first message is a brief that the
chat you are talking to wrote. A first message is not a suggestion: the new chat starts working
on it with your authority, and nobody reads it again before it does. So the consent for a
handoff cannot be the model's own proposal, however faithfully that proposal quotes the brief.
It is your harness's permission prompt, showing the exact text, in front of `charter handoff`.

## The prompt is the consent

**The rule.** On a new plane, `charter init` writes one `ask` rule for `charter handoff *`:
`Bash(charter handoff *)` under `permissions.ask` in `.claude/settings.json`, and
`"charter handoff *": "ask"` under `permission.bash` in `opencode.json`. It is the one
permission rule `init` writes. Codex has no command-pattern permissions, so there is nowhere to
put the rule for it; `init` lists nothing for Codex, and `charter doctor` names the gap.

A plane `init` did not make adopts the rule with the command its news entry names:

```bash
charter guard handoff
```

That is `charter guard ask 'charter handoff *'` with nothing to type, and either one writes the
same rule to every harness that can hold it, or to none. The short form exists because a news
entry's `adopt:` line cannot carry quotes. `charter update` does not write the rule, and neither
does anything else that runs again. Removing the rule is your decision, and a writer that ran on
every update would quietly reverse it.

**What the rule covers, measured.** Claude Code 2.1.268 was run against a local stand-in for its
model API that answered with one scripted Bash call, under a throwaway `HOME`, so no account and
none of your own configuration was involved. With the rule in the session's own
`.claude/settings.json`:

| The call | `manual`, `acceptEdits`, `auto`, `bypassPermissions` |
| --- | --- |
| `charter handoff beta <<'BRIEF'`, a body, `BRIEF` | asks |
| the same with an unquoted `<<BRIEF`, or a body holding `$(x)` and backticks | asks |
| `charter handoff beta` with no heredoc | asks |
| `FOO=1 charter handoff …`, `env charter handoff …`, `cd . && charter handoff …` | asks |
| `python3 -m charter handoff …`, `/path/to/charter handoff …` | **runs with no prompt** |

The quoted-heredoc call also asked under `default` and `plan`, and under `dontAsk` it was
refused without a prompt, so nothing ran there either. A directory with no rule ran the
quoted-heredoc, unquoted, `$(x)` and no-heredoc calls in all four columns above, and the
quoted-heredoc call under `dontAsk` and `plan` as well. "Asks" was read two ways: a print-mode
session with nobody to answer (`--permission-prompts none`) denied the call as needing approval,
and an interactive session under `bypassPermissions` showed *Permission rule
Bash(charter handoff \*) requires confirmation for this command* with the full brief above it;
declining ran nothing.

**Where the rule has to be.** Claude Code reads `.claude/settings.json` from the session's own
directory, not from above it. Measured on 2.1.268 in a plane built by `charter init`: the rule
only in the plane's file asked in a session at the plane root, and did not ask in a session at
`workspaces/<ws>/` whose generated settings held only `env`. With `permissions.ask` added to that
workspace's own file, it asked there too. A chat a frame opens stands in its workspace
directory, so that directory's file is the one that has to carry the rule. `charter doctor` run
from such a chat reads that file, and its `handoff gate` row warns when the rule is not there.

## What charter refuses that the prompt cannot cover

Inside a control plane, charter's Bash hook refuses a `charter handoff` in four situations the
prompt cannot see (the table and reasons are in [hooks.md](hooks.md), under *The guards*):

- **from a sub-agent**, when the hook payload carries `agent_id`. Measured on Claude Code 2.1.268
  and on codex-cli 0.147.0: a sub-agent's Bash call carries `agent_id`, and a main-conversation
  call does not. Whatever the sub-agent found goes back to the chat you are talking to, which
  can propose the handoff itself.
- **in an unattended run**, when the payload says `permission_mode: bypassPermissions`. The
  refusal names `charter ws todo` as the way to keep the work.
- **in any spelling but `charter handoff …`** at the start of its command, because the rule
  above did not match `python3 -m charter handoff` or a path to charter.
- **with a stdin other than one quoted heredoc** on the handoff's own segment — so the prompt
  shows exactly the text the new chat is sent.

A handoff's brief is data, not commands, to charter's secret-leak guard: a brief that names a
vault path in prose is not refused as a read of it.

## Where nothing refuses it

- **opencode.** The payload charter's plugin builds carries neither `agent_id` nor
  `permission_mode`, so a handoff from a sub-agent or an unattended run is not refused, and the
  `ask` rule in `opencode.json` is the whole gate. How opencode matches the spellings above has
  not been measured.
- **Codex.** There is no prompt in front of a handoff at all. codex-cli 0.147.0's `codex exec`
  reported `permission_mode: bypassPermissions` under its default settings, under
  `-c approval_policy=` `"on-request"`, `"untrusted"` and `"never"` (the last with
  `-s workspace-write`), and under `--dangerously-bypass-approvals-and-sandbox`, so a handoff from
  any of those is refused as unattended. Under `--approve-for-me` it reported `default`, and a
  handoff there is not refused. An interactive Codex session's `permission_mode` has not been
  measured; its handoff runs without asking unless one of the refusals above applies.

## Removing the rule

Delete `Bash(charter handoff *)` from `permissions.ask` in `.claude/settings.json`, and
`"charter handoff *"` from `permission.bash` in `opencode.json`. charter does not put it back.
`charter doctor`'s `handoff gate` row then warns `no ask rule for `charter handoff` under
claude-code, opencode`, names the command that adds it, and says the removal is your choice. It
is a warning, so `doctor`'s exit status does not change. The hook's refusals above do not depend
on the rule and stay in force without it.
