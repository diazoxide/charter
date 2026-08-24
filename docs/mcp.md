# MCP servers, personas, and credentials

## The host scopes servers per sub-agent

A sub-agent's frontmatter takes `mcpServers:` — a list whose entries are either a string
naming an already-configured server, or an **inline definition** using the same schema as
`.mcp.json`. From the Claude Code documentation:

> Inline servers defined here are connected when the subagent starts and disconnected when
> it finishes.

> To keep an MCP server out of the main conversation entirely and avoid its tool
> descriptions consuming context there, define it inline here rather than in `.mcp.json`.
> The subagent gets the tools; the parent conversation doesn't.

That is stronger than a tool allowlist. The server does not run for the session, its tools
never appear in the main conversation, and its context cost is confined to the dispatch
that needs it.

```yaml
---
name: browser-tester
description: Tests features in a real browser
mcpServers:
  - playwright:                       # inline: this sub-agent only
      type: stdio
      command: npx
      args: ["-y", "@playwright/mcp@latest"]
  - github                            # by name: reuses a session server
---
```

Inline entries support `stdio`, `http`, `sse` and `ws`.

## Credentials reach it without reaching the model

`charter secret exec` has a flag built for exactly this shape. `--exec` *replaces* charter's
process with the server rather than capturing it, so stdio streams through — a capturing
run would hang holding output from a process that never returns.

```yaml
mcpServers:
  - reddit:
      type: stdio
      command: charter
      args: ["secret", "exec", "reddit",
             "--env", "REDDIT_CLIENT_ID=client-id",
             "--env", "REDDIT_CLIENT_SECRET=client-secret",
             "--exec", "--", "uvx", "some-reddit-mcp"]
---
```

charter resolves each key from the `reddit` vault in its own process, puts the values in the
child's environment, and hands over. The file names the *keys*; the values never enter a
context window, a transcript, or a summary.

Two things that will bite:

- **`--exec` disables redaction**, because nothing is captured. Correct here, wrong for a
  command whose output you read. It is incompatible with `--file` and `--dotenv`, which
  clean up a temp file when the command returns — and an exec'd process never returns.
- **Never put values in an `env` block.** Agent files are committed. That is the failure
  this whole path exists to prevent.

## Declaring one on a persona

`charter persona sync-agents` **generates** `.claude/agents/<name>.md`, so a block written
there by hand is overwritten the next time any persona changes. charter emits it instead,
from a sidecar beside the charter:

```jsonc
// personas/reddit/mcp.json — same schema as .mcp.json, plus `secrets`
{
  "mcpServers": {
    "reddit": {
      "type": "stdio",
      "command": "uvx",
      "args": ["some-reddit-mcp"],
      "secrets": {
        "REDDIT_CLIENT_ID": "client-id",
        "REDDIT_CLIENT_SECRET": "client-secret"
      }
    }
  }
}
```

A sidecar rather than frontmatter because `persona.md`'s parser is line-based and charter
carries no runtime dependencies: nested YAML can be emitted (JSON is valid YAML) but not
read. It also means a server's own README snippet pastes in unchanged.

`secrets` maps an env var **the server demands** to a key **in this persona's vault**.
charter consumes it and emits the wrapper:

```yaml
tools: Read, WebFetch, Bash, mcp__reddit__*
mcpServers:
  - reddit: {"type": "stdio", "command": "charter", "args": ["secret", "exec", "reddit",
      "--env", "REDDIT_CLIENT_ID=client-id", "--env", "REDDIT_CLIENT_SECRET=client-secret",
      "--exec", "--", "uvx", "some-reddit-mcp"]}
```

Three things it does on your behalf:

- **Wraps the command in the persona's vault.** The generated file names the *keys*; no
  value is resolved until the sub-agent starts the server, and then only into that
  process's environment.
- **Grants the tools.** Every declared server contributes `mcp__<server>__*` to `tools:`
  when `agent-tools` is set. Otherwise the server list and the tool list are two things
  that must agree by hand, and disagreement surfaces at *dispatch* time as an error about
  "unresolved entries" — the symptom, not the cause. With no `agent-tools` the sub-agent
  inherits every tool already, so nothing is added.
- **Leaves credential-free servers alone.** No `secrets`, no wrapper: dragging a public
  server through charter would add a process and buy nothing.

Servers are **inherited and unioned** along `extends:`, parent first, child winning a name
collision — the rule `tools` and `uses` already follow.

### The wrapper is emitted only for a command you approved

`mcp.json` is committed, and it names the `command` that receives the vault's value. So
the wrapper above is emitted only once you have said, on this machine, that *this* command
may have *these* keys from *that* vault:

```bash
charter persona sync-agents               # writes the agents; names anything unapproved
charter persona sync-agents --approve-mcp # asks about each one, after showing it
```

Until then the server is still declared in the generated agent — unchanged, minus the
vault wrapper — so the persona keeps working and the server fails at authentication rather
than silently running with a credential nobody sanctioned. `sync-agents` prints the exact
command it withheld from, which is the thing worth looking at.

`--approve-mcp` asks **per server**, and prints the entry before it asks:

```
  reddit/acme → http https://api.acme.example/mcp  (env: HTTPS_PROXY)
    approve reddit/acme? [y/N]
```

Anything but an explicit yes withholds, and declining a server that was approved before
revokes it. `--dry-run` shows the same lines and records nothing. `--yes` approves every
credentialed server without asking, and is **required** off a terminal — a flag that means
yes where nobody can be asked is not consent.

The approval covers the **whole entry**, against the vault: command, args, `url`, `env`,
the `secrets`/`secret_files` mappings, and any other key the entry carries. **Change any of
them and it lapses**, because the approval is of a destination and not of a server name — a
teammate re-pointing an existing server at a new binary, a new endpoint or a new `PATH` is
the case this exists for. An entry that names neither a `command` nor a `url` has no
destination to show, so it cannot be approved at all; it is reported as withheld.

The record is machine-local under `.charter/`: if it travelled in git, the same commit that
declares a server could declare it approved.

There is deliberately no allowlist of permitted commands. An MCP `command` is an arbitrary
binary followed by arbitrary args, so any list containing the launchers real servers use
(`npx`, `uvx`, `docker`) is walked straight past by the args alone, and a list excluding
them refuses every server anyone actually runs.

### A missing vault does not block the sync

The charter is committed and shared; a vault is machine-local by design. A teammate cloning
this repo legitimately has neither the vault nor the keys, so `sync-agents` renders the
wrapper regardless — it is correct either way, and only the *run* would fail. `charter
persona lint` reports a server declaring `secrets` on a persona that names no vault. That
is ADR 0013 applied: name the divergence, do not resolve it, and do not refuse a sync on a
fresh clone.

## Caveats

- **`mcpServers` is ignored for plugin sub-agents**, deliberately, for security. charter's
  personas generate into `.claude/agents/` and charter's plugin declares no agents, so this
  works — but it would silently do nothing if personas were ever shipped as plugin agents.
- `--strict-mcp-config`, `--bare`, and enterprise managed-MCP allow/deny policies also
  cover servers declared in sub-agent frontmatter. A blocked server is skipped with a
  warning.

## Why the vault still matters more than the scoping

Scoping governs who may *call* a tool. The vault governs who may *see* a secret, and that
property survives a mistake — a transcript, a summary fed into a later prompt, a bug report
pasted into an issue. `charter secret exec` keeps the value out of all of them by never
putting it in the context to begin with.

See [secrets.md](secrets.md) for what a vault does and does not protect against — in
particular that the default provider is plaintext at file mode 0600, with no encryption at
rest.
