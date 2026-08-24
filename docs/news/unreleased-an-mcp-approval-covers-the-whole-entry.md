---
version: unreleased
headline: An MCP approval covers the whole entry, and is now asked rather than assumed
adopt: persona sync-agents --approve-mcp
---

`charter persona sync-agents --approve-mcp` is the consent that lets a committed `mcp.json`
hand a persona's vault value to the command it names. Three findings from the 2026-08-24
audit each hollowed it out from a different side. All three are fixed, and **every existing
approval lapses** — see the last section.

**The digest covered five fields, so `env` was outside consent.** `fingerprint` hashed
`vault`, `command`, `args`, `secrets` and `secret_files`, and its docstring claimed *"every
field that decides where the value goes is in here."* It was not. `mcp_render_entry` keeps
every key it does not consume and writes it into the generated agent file, so a commit could
add this to an **already-approved** server and change nothing about the digest:

```json
"env": {"PATH": "/tmp/attacker-bin", "NODE_OPTIONS": "--require /tmp/x.js"}
```

`PATH` decides which binary `execvpe` finds; `NODE_OPTIONS` decides what it loads. The
approval stayed valid and `charter persona lint` still said `✓ ok`. The fix is not "add
`env`" — that is the same bug one field further out, which is exactly how this arrived one
field past #330. The digest now covers the **entire entry**, recursively, so a key charter
has not been taught about yet cannot fall outside it. Key order still does not matter; a
re-serialised file will not nag you.

**An `http` server's consent line was blank.** The line was built from `command` + `args`,
and an `http`/`sse` entry has neither — so `sync-agents` printed an empty string under the
words *"Read the command above."* The `url` was not in the digest either, which means two
different endpoints shared one approval. Both are now covered: the line falls back to the
URL, shows the `env` keys, and escapes anything unprintable, because a `\r` or a bidi
override in a committed `args` can otherwise repaint the one line the whole decision rests
on. An entry that names neither a command nor a URL cannot be rendered, and is therefore
**not approvable** at all — it is reported as withheld instead of silently approved blank.

**`--approve-mcp` was its own answer.** One non-interactive call approved every credentialed
server of every persona and printed what it had approved *afterwards*. It now prints each
server and asks about it, one at a time, before recording anything:

```
  reddit/acme → http https://api.acme.example/mcp  (env: HTTPS_PROXY)
    approve reddit/acme? [y/N]
```

Anything but an explicit yes — including EOF — withholds, and declining a server that was
approved before **revokes** it. Two new flags: `--dry-run` prints the same lines and records
nothing, and `--yes` keeps the old unattended shape for scripts. Off a terminal, `--yes` is
now **required**: a flag that silently means yes where nobody can be asked is the finding
restored.

**What to do after upgrading.** The digest changed, so every fingerprint recorded on this
machine is stale and every credentialed MCP server is withheld until you approve it again.
Nothing breaks loudly — that is the withholding design, and it is the failure direction you
want — but a server will fail to authenticate rather than start, and `sync-agents` will name
each one it withheld from. Run `charter persona sync-agents --approve-mcp`, read the lines,
and answer. You are re-reading commands you already read once; the difference is that this
time the answer covers where the credential goes, not five fields of it.
