# MCP servers, personas, and credentials

> **Corrected.** An earlier version of this page said Claude Code could not scope an MCP
> *server* to a sub-agent, and that charter should therefore never grow an `mcp:` field.
> Both claims were wrong. Sub-agent frontmatter has an `mcpServers` field, and it does
> exactly what that paragraph said was impossible. What follows is what the host actually
> supports; charter's own modelling of it is tracked in
> [#141](https://github.com/diazoxide/charter/issues/141) and is not built yet.

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

## Until #141 lands

`charter persona sync-agents` **generates** `.claude/agents/<name>.md`, so an `mcpServers:`
block added there by hand is overwritten the next time any persona changes. Today the
options are to declare the server in `.mcp.json` — which gives up the per-persona scoping
above — or to accept that the hand-edit is temporary.

`agent-tools:` still works and is unaffected: it narrows the generated `tools:` line, and
accepts MCP patterns (`mcp__<server>`, `mcp__<server>__*`). Note that if **no** entry in a
`tools:` list resolves to a real tool, the sub-agent usually fails to launch — so naming an
MCP server that is not connected is not free.

## Caveats that survive #141

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
