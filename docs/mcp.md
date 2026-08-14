# MCP servers, personas, and credentials

charter has **no `mcp:` field on a persona**, and is not going to grow one soon. MCP
servers are declared per project or per user in `.mcp.json`, and Claude Code starts them
for the session — it does not scope a *server* to a sub-agent. So "give this persona its
own MCP server" is not a thing charter can offer, because it is not a thing the host
offers.

What charter does have is the two pieces that matter, and they compose:

1. **A credential path that keeps the value out of the model** — `charter secret exec`,
   which already has a flag built specifically for MCP stdio servers.
2. **A per-persona tool allowlist** — `agent-tools:`, which becomes the generated
   sub-agent's `tools:` line.

## 1. The server gets its secrets from a vault

An MCP server is a long-running child that speaks over stdio. `--exec` exists for exactly
this shape: it *replaces* charter's process with the server rather than capturing it, so
stdio streams through and nothing buffers output that never ends.

```jsonc
// .mcp.json — committed, so it must contain no secret VALUES
{
  "mcpServers": {
    "reddit": {
      "command": "charter",
      "args": [
        "secret", "exec", "reddit",
        "--env", "REDDIT_CLIENT_ID=client-id",
        "--env", "REDDIT_CLIENT_SECRET=client-secret",
        "--exec", "--",
        "uvx", "some-reddit-mcp-server"
      ]
    }
  }
}
```

charter resolves each secret from the `reddit` vault in its own process, puts it in the
child's environment, and hands over. The model sees the *names* — `client-secret` — and
never the values. The file names them too, which is why it is safe to commit.

Two things that will bite:

- **`--exec` is required, and it disables redaction** (there is nothing to redact — nothing
  is captured). That is correct here and wrong for a command whose output you read.
  `--exec` is incompatible with `--file` and `--dotenv` for the same reason: those clean up
  a temp file when the command returns, and an exec'd process never returns.
- **Never put values in the `env` block** of `.mcp.json`. It is committed. That is the
  failure this whole path exists to prevent.

For a server that wants a *file* of credentials rather than variables, use `--dotenv` —
but then you cannot use `--exec`, so it only works for servers that exit.

## 2. Only one persona may call it

`agent-tools:` narrows the toolset of the generated sub-agent. Omit it and the sub-agent
inherits everything; set it and it gets exactly what you list — including MCP tools, which
are named `mcp__<server>__<tool>`.

```yaml
---
name: reddit
role: Reddit Community Manager
vault: reddit
agent-tools: Read, Write, Grep, WebSearch, WebFetch, Bash, mcp__reddit__*
---
```

`charter persona sync-agents` writes that straight through to `tools:` in
`.claude/agents/reddit.md`.

### What this does not do

**It does not stop the main session, or another persona's sub-agent, from calling the same
MCP tool.** The server is running for the whole project; the allowlist narrows one
generated sub-agent, not the host. A persona whose `agent-tools` omits `mcp__reddit__*`
genuinely cannot reach it — but a session that never dispatched a persona at all is not
constrained by any persona's list.

So this is **least-privilege for dispatched work**, not a sandbox. If a server must not be
reachable at all except under one role, the honest answer today is not to declare it in a
shared `.mcp.json`.

## Why the credential still matters more than the allowlist

The allowlist governs who may *call* a tool. The vault governs who may *see* a secret, and
that is the property that survives a mistake: a transcript, a summary fed into a later
prompt, a bug report pasted into an issue. `charter secret exec` keeps the value out of all
of them by never putting it in the context in the first place.

See `docs/secrets.md` for what a vault does and does not protect against — in particular
that the default provider is plaintext at file mode 0600, with no encryption at rest.
