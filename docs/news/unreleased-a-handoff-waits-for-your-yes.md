---
version: unreleased
headline: A handoff waits for your yes, bypassPermissions included, and a sub-agent or an unattended run cannot propose one
adopt: guard handoff
---

A handoff's brief becomes a new chat's first message, and a first message runs with your
authority. The model's proposal, however faithfully it quotes the brief, is not your consent.

## The prompt is the consent

A new plane gets one `ask` rule from `charter init`: `Bash(charter handoff *)` in
`.claude/settings.json`, and `"charter handoff *": "ask"` in `opencode.json`. Measured on Claude
Code 2.1.268, that rule asks before `charter handoff … <<'BRIEF'` under `manual`, `default`,
`acceptEdits`, `auto`, `plan` and `bypassPermissions`, heredoc body and all, and under
`dontAsk` the call is refused without running. A plane that already exists adopts the rule with
`charter guard handoff`, which is `charter guard ask 'charter handoff *'` with nothing to type.
`charter update` writes nothing, because removing the rule is your choice, and
`charter doctor`'s new `handoff gate` row names it when it is missing.

## What charter refuses that the prompt cannot cover

Inside a control plane, charter's Bash hook refuses a `charter handoff`:

- from a sub-agent, read from `agent_id` (measured on Claude Code 2.1.268 and codex-cli 0.147.0
  to arrive only inside a sub-agent);
- from an unattended run, read from `permission_mode: bypassPermissions`;
- in any spelling but `charter handoff …` at the start of its command, because the rule did not
  match `python3 -m charter handoff` or a path to charter;
- fed anything but one quoted heredoc on its own segment, so the prompt shows the exact brief.

A brief that names a vault path in prose is not refused as a read of it.

## Limits

- **`python3 -m charter handoff` is refused**, and that is the spelling `CONTRIBUTING.md` uses to
  run a checkout. The rule does not match it, so allowing it would run a handoff unasked.
- **opencode cannot refuse a handoff from a sub-agent or an unattended run.** The payload charter's
  plugin builds carries neither field, so the `ask` rule in `opencode.json` is the whole gate.
- **Codex has no prompt in front of a handoff.** A sub-agent's call and a run reporting
  `bypassPermissions` are refused; `codex exec` reported that under every approval policy tried
  except `--approve-for-me`, which reports `default`. Any other Codex handoff runs without asking.
- **A chat reads the settings of the directory it stands in.** A chat a frame opens stands in
  `workspaces/<ws>/`, and the rule asks there only once that directory's own
  `.claude/settings.json` carries it. `charter doctor`, run from that chat, says whether it does.

`charter docs show handoff` has the measurements and how they were taken.
