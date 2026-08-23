# Charter security + completion audit — master tracker

**Created:** 2026-08-24 · **Owner:** steward · **Status:** in progress

The single place that says what is outstanding. Written because this session has
accumulated more open threads than one context can hold, and dropping one silently
is the failure mode that matters.

## A. External security review (2026-08-22, v0.48.0, commit 3284762)

A third-party reviewer audited sources, hooks, harness adapters, secret providers,
the release workflow and tests. They ran the suite (3274 pass), built and
`twine check`ed both artifacts, ran `pip-audit` (charter has no runtime deps) and
Bandit (62: 61 Low, 1 Medium — `urlopen` of a fixed PyPI HTTPS URL, a false
positive). No `shell=True`, `eval`, `exec`, `pickle`, or unsafe YAML loader.

Their scores: accidental disclosure ~6/10; prompt injection / mistaken model 2/10;
malicious or compromised model 1/10; **not suitable for infrastructure master
passwords without a separate broker/sandbox**.

### The 13 claims — every one needs a verdict with evidence

| # | Claim | Verdict | Evidence |
|---|---|---|---|
| 1 | The model can still obtain a secret via ordinary `secret exec` | **CONFIRMED** | `commands_secrets.py:616-649` states in-code "Output is NOT redacted" for `--exec` and `--stream`; no PreToolUse hook denies them |
| 2 | Literal redaction is trivially bypassed by transforming the value | pending | |
| 3 | The child gets the harness's WHOLE environment, not just the requested secret | pending | |
| 4 | `secret cp` can materialise a secret anywhere | pending | |
| 5 | A persona is not a real vault access boundary | pending | |
| 6 | The opencode adapter does not wire the specialised vault read guard | pending | |
| 7 | The shell guard is bypassed by plain Python | pending | |
| 8 | Tool auto-approval is too broad | pending | |
| 9 | A secret's hash and length are shown to the model | pending | |
| 10 | Plain-file vault stays plaintext (JSON, 0600, no encryption at rest) | pending — reviewer notes the project already documents this honestly | |
| 11 | Temporary secret files are not shredded | pending — code DOES shred in `finally`; the documented limit is a SIGKILLed parent | |
| 12 | MCP approval confirms the command line, not the true recipient | pending | |
| 13 | Hooks fail open in several places | pending | |

**The structural criticism, which outranks all thirteen:** arbitrary `secret exec`
lets the *model* choose both the secret and the receiving process. A sound design
binds a secret to a pre-approved operation or a specific immutable executable.
Redaction is irrelevant when the child can simply post the value to the network.

**Standing instruction from Aaron:** *"this all points you should check verify -
grill first then implement."* Refuted claims get reported too — he is dealing with
the reviewer and needs to know which criticisms do not stand.

## B. Open GitHub issues

| # | Title | State |
|---|---|---|
| 386 | frame v2 (2/3): the frame owns the surface | agent in flight |
| 387 | frame v2 (3/3): density presets + animation | agent in flight |
| 411 | panels do not follow a workspace switch | routed into #386 |
| 395 | `charter guard` shows opencode's rule as a Python tuple | open |
| 399 | `secret set` reports a 1Password secret missing after writing it | open |
| 400 | docs/secrets.md documents a removed 1Password schema | open |
| 401 | plane-root guard stops a branch switch but not `git reset --hard` | open |
| 402 | the suite runs `frame.state.reap` against the real plane | open |
| 408 | a panel dying inside the operator's own tmux never respawns | open |
| 409 | frame signal-death integration test is flaky on tmux 3.4 | open |
| 358 | awesome-ai-plugins listing request | **Aaron's own call — do not action** |

## C. Standing quality rules earned in this session

- **Five flavours of test-that-cannot-fail** have been hit here: an ambient env var
  already set; a fix invalidating its own sibling test; a fixture coincidence; a
  fallback masking the mutation; and a fixture VALUE the fix gave new meaning.
  Mutation-test every new guard: apply, confirm RED, restore, confirm GREEN.
- **pid 1 is launchd, permanently alive.** Frame ids end in a pid, so any fixture
  named `something-1` is read as live and its assertion becomes unfailable.
- **`PersonaIso` for anything touching plane state** — see #402 for what happens
  without it.
- CI is Ubuntu / tmux 3.4 / `TERM=dumb`; dev is macOS / tmux 3.7c. Probe
  capability, never presence. Never assert on tmux's own message prose.
- Charter defects go upstream: file the issue, do not patch around it locally.
