---
version: unreleased
headline: A child persona can override its parent's MCP server again
---

`mcp_servers()` documented the rule — *"parent first, child wins on a name collision"* — and
did the opposite. It iterated the inheritance chain child-first into a `dict.update`, so the
most distant ancestor overwrote everything below it.

A child that redeclared a server name its parent already used could not override it. Nothing
reported this: the child's entry was read, applied, and then overwritten, so
`charter persona sync-agents` succeeded and the generated sub-agent quietly carried the
ancestor's config. The only symptom was a server behaving like somebody else's.

Fixed. If you have an `extends:` chain where two personas declare the same server name, the
**child's** entry now wins — which may change which config a persona actually launches. Check
with:

```bash
charter persona sync-agents
git diff .claude/agents/
```

A plane with no name collisions in an inheritance chain sees no change at all.
