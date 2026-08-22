# Policy that fits a pattern belongs to the host

Charter guards Bash calls through a `PreToolUse` hook: it denies a command that would leak
a secret, denies a branch move in the plane root, denies SSH and signing, asks before a
commit inside a clone, and allows a binary the active persona declares.

The obvious next step — letting a plane list its own commands to force-prompt, in
`charter.toml` — is the one this rejects.

**Policy that can be written as a command pattern belongs to Claude Code's `permissions`.
Charter keeps only the policy that needs context the host cannot see.**

## Why

Claude Code already has the feature, and has more of it than charter would build:

* `permissions.ask` sits between `deny` and `allow`, evaluated in that order, first match
  wins.
* Rules live in the plane's own `.claude/settings.json`, which is **committed**, so every
  engineer on the repo gets the same list with nothing to install or sync.
* It segments compound commands correctly. The recognised separators are `&&`, `||`, `;`,
  `|`, `|&`, `&` and newlines, and *"a rule must match each subcommand independently"* —
  which is precisely the job `hooks._segment_argv` does by hand, and got wrong once when a
  `shlex` fallback collapsed a command into a single token and blinded the leak guard.

A `charter.toml` list would therefore be a second policy engine for a job the host does
better, and would have to re-implement shell segmentation to be safe. Worse, the two
engines cannot be made to agree: **a hook cannot relax a permission rule.** The
documentation is explicit — *"a matching ask rule still prompts even when the hook returned
`allow` or `ask`"* — so charter's own decisions are already subordinate to the host's, and a
second list inside charter would be advisory over a mechanism that outranks it.

## What charter keeps, and why that is the whole line

The four guards split on one question: can the rule be stated without knowing where you are
standing or who you are?

| guard | needs | expressible as a rule |
| --- | --- | --- |
| `_leak_reason` | argv, and the plane's vault paths | no |
| `_plane_root_branch_reason` | the working directory | no |
| `_clone_commit_reason` ⁺ | the working directory | no |
| `toolgate.decide` | the active persona's declared tools | no |
| `_single_credential_reason` | the command alone | **yes** |

⁺ Removed in #371 — not because the line drawn here moved, but because that particular
guard failed a different test: it asked 471 times in one plane, was approved 97 times out of
98, and its trigger condition was the workflow `skills/working-in-a-clone` prescribes. "Not
expressible as a host rule" answers *where a guard lives*. It never answered *whether the
guard should exist*, and this table was read as if it did.

Only the last is static, and it deliberately stays in the hook anyway. A native `deny` rule
prints no reason, and the reason is most of what that guard is for: a developer who reads
*"one credential — each forge's token over HTTPS; no SSH, no signing"* learns the rule,
while one who reads a bare refusal files a bug. Charter trades the scoping win for the
explanation.

That trade is not free and is worth restating: because the guard is not project-scoped, it
needs `config.HAS_CONTROL_PLANE` to stay silent outside a plane — a gate added after it
fired in unrelated repos and explained a control plane that did not exist there.

## What this permits charter to do

Write the host's rules, never keep its own. `charter guard ask <pattern>` edits
`permissions.ask` in the plane's `.claude/settings.json` — the same file charter already
maintains for the status line and the plane-root guard. There is no charter-side list, no
sync step, and nothing that can drift, because there is only one record.

## Consequences

* A plane's force-prompt list is portable to anyone using Claude Code, with or without
  charter.
* Charter must **detect** the one interaction this creates rather than assume it away: a
  broad ask rule shadows `toolgate`'s `allow`, so a persona's declared tools start prompting
  and nothing says why. `doctor` names the overlap. A mechanism that looks wired and is not
  is the failure shape this repo keeps paying for (#177, #197).
* If Claude Code ever gains directory-scoped rules, three more guards become expressible and
  this decision should be revisited rather than defended.
