# The hooks

The CLI is one of charter's two artifacts. The other is a **Claude Code plugin**, and the
plugin is almost entirely hooks: the things that have to happen without anyone remembering
to ask for them.

Install both — a CLI with no plugin leaves every guard and every injection dead, while
looking completely installed. See [install.md](install.md).

The plugin ships **no Python**. Every hook is a `charter hook <name>` call against the CLI
on `PATH`, which is why a version skew between the two is worth shouting about and is the
one thing a hook is allowed to shout about.

## What fires, and when

| Event | Matcher | What it does |
| --- | --- | --- |
| `SessionStart` | — | reconcile workspace state, GC persona scratch, inject context, run `doctor`, refresh forge state |
| `UserPromptSubmit` | — | the commitment gate (below) |
| `PreToolUse` | `Bash` | the four guards (below) |
| `PreToolUse` | `Read\|Grep` | keeps a vault file from being read into context |
| `PreToolUse` | `Task\|Agent` | notes a dispatch about to happen |
| `PostToolUse` | `Write\|Edit\|MultiEdit` | the record-memory nudge |
| `PostToolUse` | `Skill` | tallies which skills a persona actually invokes |
| `PostToolUse` | `Task\|Agent` | tallies the dispatch |
| `Stop`, `SubagentStop` | — | autosave a LIVE workspace |

## The guards

A denial from these is **the rule working, not a bug** — the single most common thing
mistaken for a defect. Each prints why, because a developer who reads the reason learns the
rule while one who reads a bare refusal files an issue.

- **Secret leak.** A command whose argv would put a vault's contents into the transcript.
  Needs argv *and* the plane's vault paths, so it cannot be expressed as a static rule.
- **Plane-root branch move.** The plane is not a work tree (ADR 0008); a branch switch there
  is almost always meant for a clone.
- **One credential.** SSH to a forge, `GIT_SSH_COMMAND`, `-S`/`--gpg-sign`, and the
  `core.sshCommand` family that reaches the same transport by another road (`-c`,
  `--config-env`, `GIT_CONFIG_KEY_n`, and a `git config` write of it). This one *is*
  expressible as a pattern and stays in the hook anyway, so it can explain itself — see
  [git-policy.md](git-policy.md) and ADR 0014.
- **Commit inside a clone.** Asks rather than denies: committing there is usually intended,
  and only the working directory reveals which case it is.

A fifth path is not a guard but an allowance: a binary the **active persona** declares in
`tools:` runs without a prompt while that persona is active, and only then.

Policy that *can* be written as a command pattern belongs in Claude Code's own
`permissions`, not here — `charter guard ask <pattern>` writes it there. Charter keeps only
what needs context the host cannot see. That line is ADR 0014.

## What gets injected

`SessionStart` puts a bounded amount of context in front of the session: the active
persona's role, a digest of memory (a digest, not the corpus — recall is a search, not a
preload), the workspace to confirm, and a warning when the plane's config has moved on
since the session began.

The `UserPromptSubmit` gate is narrower than it sounds. It fires when a prompt asks for
work *and* carries a real fork — open-ended, broad, destructive, or multi-part — and its
effect is to say: scout first, then ask, before dispatching or editing.

When the acting persona declares `routing: advise` or `require`, the same message leads
with the **roster** — who else exists, what each advertises, when each was last dispatched.
One message, not two: two blocks on one prompt is how a nudge becomes wallpaper. charter
never says which persona owns the prompt (ADR 0016); see `docs show personas`.

## What gets counted

Two tallies — tracked, not ignored, though charter never commits them for you (`[memory].share` defaults to `local`) — both counts-and-dates only, never prompt text, and both
parallel-writer safe with the host in the filename so two engineers never conflict:

- **Dispatches** (`personas/_dispatch/`) — was this persona ever actually used, or did its
  work quietly route to a generic agent? Surfaces in `charter persona stats` as the
  `⚑ never dispatched` flag and the persona-vs-generic ratio.
- **Routing advice** (`personas/_dispatch/`, as `{"ts", "event": "advice"}` rows) — how
  often the roster was shown. Paired with dispatches in `charter persona stats`, it is the
  number that can say the block is not working.
- **Skill invocations** (`personas/_skills/`) — a persona's declared `skills:` are preloaded
  into every dispatch of it, so declaring one is cheap to write and expensive to keep. This
  says whether the equipment was worth carrying.

Neither has a secret surface to scan, by construction.

## When a hook fails

Hooks swallow their exceptions. A tally that breaks a turn is worse than a tally that
misses a row, and none of this is load-bearing for the work itself.

The deliberate exception is version skew: a stale CLI would stop firing the gate while
everything still looked installed. That is the failure shape this project keeps paying for,
so it is the one thing a hook says out loud.

Seeing what actually happened: `charter trace` reports guard denials, tool approvals, secret
warnings and memory writes for the session.
