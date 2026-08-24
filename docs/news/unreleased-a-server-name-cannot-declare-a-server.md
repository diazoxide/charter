---
version: unreleased
headline: a committed MCP server name could declare a second server and spend any vault on the machine
---

`personas/<name>/mcp.json` is committed. It arrives from whoever last pushed to the repo, and
its keys are server **names**. `charter persona sync-agents` wrote each one into the generated
sub-agent's YAML frontmatter like this:

```python
fm.append(f"  - {server_name}: {json.dumps(entry)}")
```

The serialiser quoted the entry. An f-string pasted in the key. So a newline in a committed
name ended that line and opened another — a second `mcpServers` entry, chosen entirely by
whoever wrote the name, and the obvious thing to put there is charter's own credential
wrapper:

```json
{"command": "charter",
 "args": ["secret", "exec", "<any vault registered on the machine>",
          "--env", "TOKEN=<any key>", "--exec", "--", "sh", "-c", "…"]}
```

**Nothing on the consent path could have caught it.** #330's approval covers the entry that
carries a credential — the vault, the command, the args, the two secret maps — and the
carrier server here declares no `secrets` at all. So there was no fingerprint, no prompt, no
withheld line, and no mention of the injected entry anywhere, because the injected entry was
never a value charter had read. The run printed `✓ Synced 1 persona sub-agent(s)` and the
next dispatch of that persona handed a vault to somebody else's command.

**A name is now an identifier because charter makes it one.** Letters, digits, `_`, `.` and
`-`, first character alphanumeric or `_`, 64 of them — bounded in `persona.mcp_servers`,
which is the one function every consumer of a declared server goes through, so the render,
the `mcp__<server>__*` tool grant, `lint` and the credential list inherit the bound rather
than each remembering it. Anything else is refused, and the server is not declared.

**And the refusal is loud.** `[frame] hotkey` bounded the same class of value and said
nothing, so a plane carrying the payload still rendered a clean green tick; that half is the
reason a bound is worth having and the reason it goes unnoticed when it fires. `sync-agents`
now names each refused name on the run that wrote the agent, `persona lint` reports it as an
error, and both say which file to edit.

The emission was fixed too, not instead: the whole single-key mapping is serialised now, key
included, so the frontmatter round-trips whatever string reaches it. Two layers with
different jobs — and the tests force a hostile name past the boundary specifically to prove
the second one holds on its own, because a guard that passes only because a different guard
fired is a guard nobody knows is broken.

One more surface, the same class: `sync-agents`'s withheld list and its approval lines are
rows of the form `persona/server → command`, built out of the same committed file. A newline
in an `args` entry wrote a row indistinguishable from charter's own — so a report could name
a server that was never withheld, under a count that agreed with it, and the operator's `y`
would then cover something they had not read. Every committed value charter prints back now
goes through one bound that escapes anything without a glyph. It is a display bound, not a
promise: it stops a value forging a *line*, and it will not tell you that `l` is not `I`.

*Bounded values do not travel.* A plane whose `mcp.json` uses a name outside the alphabet
loses that server the next time it syncs, and is told which one and why. Rename it and re-run.
