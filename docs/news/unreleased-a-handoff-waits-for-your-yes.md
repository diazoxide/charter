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
- in a spelling it can recognise as other than `charter handoff …` (a wrapper, a path, a word
  quoted or escaped, a word that reads `charter` or `handoff` once its expansion characters are
  removed, odd spacing), or inside a string or heredoc a shell runs (`eval`, `bash -c`,
  `bash <<'EOF'`), because the rule did
  not match `python3 -m charter handoff`, a path to charter, `charter 'handoff'`,
  `charter $'handoff'` or a handoff inside `bash -c` or a `bash <<'EOF'` body;
- fed anything but one quoted heredoc on its own segment, so the prompt shows the exact brief.

A heredoc body is searched when its own opener is a shell or an interpreter (`bash <<'EOF'`,
`python3 <<'PY'`, `ssh`), or when charter cannot name the opener. Every other opener hands its
body on without running it, so `git commit -F -`, `tee`, `mail` and every reader keep their
bodies as data — each heredoc judged by the program that opened it, so
`( cat <<'A' > notes.md; bash <<'B' )` searches only `bash`'s. A `<<` inside quotes opens
nothing; a `<<` inside `$( … )` opens a heredoc even when quotes surround it.

A brief is no longer refused as a read when it names a vault path in prose, when it holds
an apostrophe, or when one of its lines opens with a reader (`cat .charter/vaults/dev.json
would print it, so never run that.`) — the briefs a chat writes to warn the next chat off a
secret.

## Limits

- **The hook reads a command's words; it is not a shell.** A handoff run by an interpreter
  (`python3 -c`, or `os.system` inside a `python3 - <<'PY'` body), through a variable, from a
  script file, behind an expansion that does not leave
  the word whole (`charter {hand,}off`, `charter $'\x68andoff'`, both of which ran with no prompt),
  or more than one string deep is not seen. Claude Code's docs
  say the same of its own rule: it "isn't a security boundary around the program".
- **One apostrophe can switch the look inside shell strings off.** A heredoc body that is not a
  reader's — a `python3 - <<'PY'`, `git commit -F -` or `tee` body — holding a lone `'` leaves
  the call unparseable, and a handoff in a later `eval '…'` or `bash -c '…'` is then allowed.
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
