# autonomy

> **Living charter** for this workspace — its north star and shared context.
> Keep it current as the work evolves (edit this file, or `charter workspace vision "…"`).
> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone
> can pick up the task with full context. Never put secrets here (vault only).

## Vision

**charter should stop having an opinion about autonomy.** The opening question was
"how do we make a job autonomous — is the holder the workspace, the persona, or the
session?" Grilling dissolved it: the holder is **the harness**, which already declares
`permission_mode` in the hook payload on both Claude Code and Codex. charter needs no
autonomy mode, no `charter.toml` switch, no launcher, and no new concept.

What remains is small and evidence-shaped: **delete the one guard that has ever
interrupted anybody, teach charter to measure its own guards, and stop charter's own
injected prose from telling an agent to ask a question when nobody is there.**

## Context & decisions

### The three-way question, answered

| Axis | Holder | Why |
| --- | --- | --- |
| **The job** (durable intent) | workspace | unchanged |
| **Capability** (what may run) | persona (`tools:`) | unchanged |
| **Autonomy** (is a human there) | **the harness** | it already declares it; charter reads, never sets |

Durable carriers were rejected because they make autonomy **sticky** — set it on a
workspace/persona/plane and an attended session inherits it silently tomorrow. A
`CHARTER_AUTONOMOUS` env var was drafted and then dropped: `permission_mode` is already
in the payload (`auto` / `bypassPermissions` / `default` / `plane`), and charter's own
steward memory records Codex requiring the same field name. Reading a field the harness
declares is not the tty-sniffing that was rejected — that distinction was initially
conflated, then corrected.

### Why the harness cannot simply be delegated to

Claude Code changelog: *"Fixed auto mode overriding a PreToolUse hook's `ask` decision
for unsandboxed Bash — **a hook `ask` now floors the decision at a prompt**."* A hook
`ask` outranks every harness permission mode, deliberately. So "delete charter's git
hooks and let the harness handle it" **cannot** fix an ask.

And the denies cannot be delegated at all, because the split is **pattern vs state**:

- The harness decides on patterns — `Bash(git push:*)` globs in settings.
- `_plane_root_branch_reason` needs to know where the plane root is, whether cwd is
  inside it, and runs `git symbolic-ref` to read the branch. `_single_credential_reason`
  needs `_known_forges()` resolved from **this plane's** `charter.toml`, including a
  self-hosted host declared today.

Neither is expressible as a glob. Not duplication — a question the harness structurally
cannot answer.

### `ask` is the enemy; `deny` is harmless

A deny returns instantly and the agent routes around it. An ask **blocks on a human who
is not there**. This is the axis the whole design turns on.

### Evidence (23 traced sessions in this plane, 2026-08-08 .. 2026-08-19)

| event | count |
| --- | --- |
| **ask** — clone-commit | **231** |
| ask — dispatch-overlap | **0 (never fired)** |
| ask — routing | **0 (never fired)** |
| deny — single-credential | 285 (279 `git`, 6 `ssh`) |
| deny — secret `--reveal` | 278 (all `charter`) |
| deny — plane-root-branch | 7 |
| deny — vault read | 6 |

Across all three planes on this machine (transcript scan): **335 single-credential
denials in ten days** — 181 `easydmarc-umbrella`, 101 `charter`, 53 `volaticloud`.

### Hypotheses tested and KILLED — do not re-run these

- **Test-suite pollution.** Disproven. Every test invoking the tracing handler
  subclasses `PersonaIso`, which redirects `PERSONA_STATE_DIR` via `config.use()`. The
  unisolated cases (`LeakCase`, `TestAbbreviations…`) call `_leak_reason` directly, which
  does not trace.
- **Guard firing outside a plane (the regression `HAS_CONTROL_PLANE` was added for).**
  Disproven. `easydmarc-umbrella` and `volaticloud` each have their own `charter.toml`
  and `.charter/`. They are real planes; the gate works. **An earlier read that the guard
  was "miscalibrated" was wrong** and is recorded here so it is not re-derived.

### The open thread that matters most

`_single_credential_reason`'s docstring claims *"these denials only catch a **deliberate**
bypass."* Nobody deliberately bypasses 335 times in ten days. Something in normal
workflow keeps emitting SSH-or-signing git commands. **charter cannot say what**, because
`_trace` records only the binary (`cmd=head`). Fixing that is the prerequisite for
fixing anything else.

### Settled decisions

1. **No autonomy mode, no config, no launcher.** The only autonomy-aware line in charter
   is: `_ask()` must not emit `ask` when `permission_mode == bypassPermissions`. Keyed on
   `bypassPermissions` alone — `auto` usually *does* have a human watching, and being
   wrong toward asking costs a prompt while being wrong toward silence costs a guard.
2. **The hard floor is non-overridable**, by any permission mode or config: secret leak,
   outward action under the operator's identity (ADR 0003), plane-root branch move (#157).
   `bypassPermissions` means *stop asking me*, not *stop knowing things*.
3. **No `charter.toml` override for denies.** A guard that gets in the way is
   miscalibrated, and that is fixed upstream. An override switch is a pressure valve that
   destroys the evidence the guard was wrong — the 335-denial finding above is exactly the
   evidence such a switch would have deleted.
4. **No per-guard config table.** It is the same override switch wearing a different hat.
5. **charter writes the host's rules and keeps no list of its own** (ADR 0014, already
   settled — `charter guard ask` is the existing precedent). A `charter.toml` list would
   be "a second engine that could not win."
6. **No charter-side circuit breaker for retry loops.** `maxTurns` is the harness's, and
   `commands_persona.py:591` already forbids charter touching `permissionMode`/`maxTurns`.
   Surface repeat-denial counts in `doctor` instead — a guard tripped 335 times is a
   finding, not an incident to auto-abort.
7. **Rewrite the injected prose; do not intercept `AskUserQuestion`.** Interception
   (changelog: a PreToolUse hook can satisfy an `AskUserQuestion` via `updatedInput`)
   would have charter answering design questions on the operator's behalf — against
   ADR 0016. Rewriting keeps judgement in the model and accountability in the trace.
8. **Deleting the clone-commit ask is CONDITIONAL on outcome data.** 231 emissions is not
   231 approvals; charter never records how an ask resolved. If it is approved ~always it
   is a tax and goes; if declined often it works and stays.

### Deliberately parked

- `"defer"` PreToolUse decision (purpose-built for headless pause/resume) — needs
  `-p --resume` orchestration that does not exist here. Revisit only if a launcher is
  ever built.
- Precedence of a settings `permissions.allow` against a hook `deny` is **unverified**.
  Mitigated by restricting any generated allow-list to read-only verbs, which is safe
  under either answer.

## Implementation status — CLOSED · shipped across 0.46.0 – 0.46.3

| # | What landed | Where |
| --- | --- | --- |
| 288 | `trace._session()` delegates to `session.bucket()` | `charter/trace.py` |
| 289 | `_single_credential_hit()` — one scanner behind both the prose and a `shape` field | `charter/hooks.py` |
| 290 | `_ask` marker + `posttooluse-bash` handler → `ask-approved` | `charter/hooks.py`, `hooks/hooks.json` |
| 291 | `charter guard allow` mirroring `guard ask`, all harnesses | `charter/commands.py`, `charter/harness/*`, `charter/cli.py` |
| 292 | `_unattended()` → `_ask` allows; prose rewritten; workspace-confirm fails fast | `charter/hooks.py` |

Suite: **2639 → 2706 tests, green** (67 added, none loosened), and green again under the
runner's git config per CONTRIBUTING.

**Shipped as:** branch `autonomy-guard-observability` in `workspaces/autonomy/charter`,
commit `802f96b`, 0.45.0 → 0.46.0 (5 things: 4 version files + `charter news stamp`),
captures regenerated. **PR #293 — all four Python versions pass.**

**Released.** PR #293 squash-merged as `2a3f6d6`, `main` CI green (checked separately per
CONTRIBUTING), tagged `v0.46.0`, `release.yml` succeeded, published to PyPI via Trusted
Publishing, GitHub Release created by the announce job. All three features smoke-tested
against the *published* binary, not the working tree.

The merge and the `/auto-mode-setup` re-run were done by the operator: Claude Code's auto
mode classifier denied `gh pr merge` and denied the agent editing its own permission config,
which is the same rule `commands_persona.py:591` already applies to persona charters — an
actor may not widen its own permissions. Not worked around.

### Two bugs found by RUNNING it, not by the tests

Both were invisible to unit tests that asserted the intended behaviour and nothing around it.

1. **`cmd=head` leaked operand values.** `cmd.split()[0]` looks like a binary name, and for
   `VAR=value git push` it is the whole assignment. Live traces held
   `D=/private/tmp/…/demo-plane;`; a `GIT_SSH_COMMAND=/keys/id_rsa` would have landed the
   same way — the exact leak the guard beside it exists to prevent, in a file that outlives
   the conversation. Now `_trace_head()` keeps the variable NAME only.
2. **An unattended ask was counted twice.** `_ask` traced `ask-unattended` and the call site
   then traced `ask` unconditionally, so the tally that exists to separate "asked" from
   "would have asked" recorded both. `_ask` now returns whether it actually asked.

### Known wart

`guard allow`'s opencode line prints a tuple — `allowing ('bash', 'git status *')` — because
opencode's rule is `(tool, glob)` rather than a string. Pre-existing: `guard ask` has printed
it the same way since it shipped. Left consistent rather than fixed in one of the two.

### The sixth issue — #299, found by turning the question on the work

Asking *"what would an autonomous agent do with this same task?"* found a regression 0.46.0
had just introduced. `_clone_commit_reason` matches `_GIT_WRITE_RE`, which includes `tag` and
`push` — so `git tag v0.46.0` from a clone used to return `ask`, and a hook `ask` floors at a
prompt in EVERY mode. It stopped unattended releases **by accident**. 0.46.0 turned it into
`allow`, and pushing a tag fires an irreversible PyPI publish. `gh pr merge` / `gh release
create` were never guarded at all, being no kind of `git`.

Fixed in **0.46.1**: publishing is on the floor — unattended, charter denies tag creation, tag
pushes, and `gh`/`glab` release-create and merge. Deny rather than ask, because an unattended
ask IS an allow now. Keyed on tagging rather than a `v*` shape, because shape is walked past
by naming the tag `release-1`. Attended behaviour asserted unchanged, as hard as the denials.

**The durable lesson: when converting an `ask` into an `allow`, enumerate what that ask was
INCIDENTALLY covering.** We reasoned carefully about what the nudge was *for*, and never asked
what else it was catching.

## Closing note — what this workspace actually covered, and what it should not have

**Delivered (autonomy, its real subject):** #288–#292 in 0.46.0, and #299 in 0.46.1 — the
release floor, found by asking what an autonomous agent would do with the task that produced
0.46.0.

**Absorbed but off-subject, recorded so a reader is not misled:** #301 `guard allow --local`
(0.46.2) and its follow-up defect #305 (0.46.3) are permissions ergonomics; #302 was a flaky
test unrelated to any of it. They arrived through a genuine causal chain — each was found by
verifying the one before — but they are not autonomy, and a charter that describes a grab-bag
is worse at its only job.

**Deliberately not carried here:** #306, the one-directional plugin-skew guard. It wants its
own workspace and a clean charter.

### Process lessons, worth more than the code

* **Batch releases.** Four in one day meant three hurried verification passes instead of one
  careful one, and 0.46.2 shipped broken.
* **Route release cycles to the `release` persona.** It holds exactly the knowledge that bit
  us — the PyPI index-lag lie, the five version points, the asset-freshness gate.
* **Split by subject, not by discovery.** The causal chain belonged in one session; the
  workspace should have forked when the chain left the vision.
* **Verify in the field, not only in tests.** Every finding that mattered came from running
  the published thing: the `cmd` operand leak, #299, the `--local` gitignore defect, and the
  plugin sitting two minors behind while `doctor` ticked green.

## Glossary

- `ask` — a hook `permissionDecision` that blocks waiting for a human. Outranks every
  harness permission mode; the thing autonomy actually trips over.
- `deny` — a hook refusal. Returns instantly; harmless to an unattended run.
- **the floor** — the guards no permission mode or config may unlock (decision 2).
- **pattern vs state** — the line between what the harness can guard (command globs) and
  what only charter can (plane root, declared forges, cwd).
- **plane** — a directory with its own `charter.toml` + `.charter/`. This machine has
  three; each traces separately.

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).
