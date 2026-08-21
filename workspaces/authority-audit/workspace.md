# authority-audit

> **Living charter** for this workspace — its north star and shared context.
> Keep it current as the work evolves (edit this file, or `charter workspace vision "…"`).
> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone
> can pick up the task with full context. Never put secrets here (vault only).

## Vision

Where does input someone else controls reach authority charter holds? The invariant: parsing shared data must never execute anything and never reach a credential. Threat model, not ergonomics; findings before fixes.

## Context & decisions

<!-- Key facts, constraints, and design/architecture decisions found while working —
     the durable "why", not a chronological log. Grow this as you learn. -->

### The invariant (settled with the operator, 2026-08-21)

> **Parsing shared data must never execute anything, and must never reach a credential.**

Deliberately precise. A teammate who can commit *code* to a repo you run does not need a
charter file to attack you — **code is not the boundary, data is.** "A malicious PR could
change charter's source" is true of every project and is out of scope.

Settled alongside it: threat model, not ergonomics · charter files first, forge data second ·
declared authority (the `permissions.ask` rules charter writes) as a later pass · **findings
before fixes**, with a stop in between · hostile author assumed · **demonstrate, don't reason**
(reasoning says "could", a canary says "did") · public issues describing mechanism, never
recipes.

### The root cause under most of it

**charter validates a name when a HUMAN TYPES IT — `workspace create`, `persona create`,
`worktree`, `docs show` — and validates nothing when it reads the same name OUT OF A COMMITTED
FILE.** `valid_name` exists in both `persona.py:41` and `workspace.py:81` and is called from
six places, none of them a parser. Findings 1, 6, 7 and 9 plus forge F3 are that one omission
wearing different file extensions.

The forge half has its own version of the same shape: `glstate` and the forge protocol were
written against **failure** — everything degrades to `_EMPTY`, nothing raises, the line never
crashes — and not against **malice**. GitLab's backend percent-encodes; GitHub's `ci_status`
does not, eight lines below an `open_change` that does. That asymmetry is an accident, not a
policy, and it landed on the one call that runs unprompted every two minutes.

### Findings (15) — full detail in the GitHub issues

**Tier 1, data → credential or execution:** a branch name making `gh api -F` dereference `@` as
a filename and post the contents in an authenticated request (unprompted, every 2 min) ·
committed **symlinks** followed everywhere, routing around charter's own `pretooluse-read` vault
deny and into an agent's system prompt, and hanging `doctor`/`statusline`/`sessionstart` via a
FIFO · `mcp.json` rendering to `secret exec … --exec -- <command>` with the vault value in the
child env (#317's shape, different file) · a committed `tools:` grant emitting `allow`, with
`uses:` accepting a path that escapes the plane while lint calls it dangling · `charter.toml`'s
`version` installing any published charter at SessionStart, **downgrades included** — which
re-opens #317 on every teammate's next session.

**Tier 2, traversal into credentialed git:** manifest and inventory repo names · an npx package
spec from `vaults.json` · a vault pointed at any absolute path, silently chmod-ed by `doctor` ·
`extends:` accepting a path.

**Tier 3:** `role:` spliced into the SessionStart briefing with imperative framing and **no data
label**, while every other injected block carries one · control characters surviving the render
path (`tui._SGR` matches only `\x1b\[[0-9;]*m`, so `\x1b[2J`, OSC and BEL pass through and are
counted as visible width) · five smaller committed-config items.

### Guards that exist and must not be refactored away

No `shell=True` anywhere; every child is a list argv through `util.run` with
`GIT_TERMINAL_PROMPT=0` · `resolve_host` fails closed (nine host-confusion URLs all returned
unmanaged) · the #317 fix **generalises** — `_pass_through` reads open-ended positionals off the
argparse parser rather than naming `secret exec`, so the hole cannot return under another
command's name · `sync-agents` takes output filenames from a directory listing, never from
frontmatter · `clamp_share` fails to `local` · SessionStart never injects a persona's charter
body · a `vault:` naming a path is inert (registry lookup, not a path join).

### One correction to the brief, and it was right

`_ask` (`hooks.py:87-112`) **downgrades to `allow`** under `bypassPermissions` with an explicit
"unattended, not blocking" reason, so hostile committed data cannot floor an unattended operator
through it. Denies are deliberately not downgraded. The "blocks the operator" axis is the
symlink hang instead, plus a narrow `[[forge]] host` deny widening.

### Sequence (operator's call)

1. Encode the forge argv (one `quote(safe="")`, matching the line eight above it) — assigned.
2. **`valid_name` at every parse boundary** — closes four findings at once.
3. Symlink containment. 4. Timeouts on forge CLI calls. 5. A data label on `role:`.

## Glossary

<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->

_Nothing yet._

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).
