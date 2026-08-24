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
  - {"reddit": {"type": "stdio", "command": "charter", "args": ["secret", "exec", "reddit",
      "--env", "REDDIT_CLIENT_ID=client-id", "--env", "REDDIT_CLIENT_SECRET=client-secret",
      "--exec", "--", "uvx", "some-reddit-mcp"]}}
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

### A server name is an identifier, and charter enforces that

Letters, digits, `_`, `.` and `-`; 64 characters; the first one alphanumeric or `_`. A name
outside that alphabet is **refused**, the server is not declared, and `sync-agents` and
`charter persona lint` both say which name and which file.

That is narrower than JSON allows, on purpose. The name is emitted as a key in the
generated agent's YAML and as `mcp__<server>__*` in `tools:`, and it used to be pasted into
both rather than serialised — so a newline in a committed name ended the YAML line and
declared a **second** server, entry and all, which could be `charter secret exec` against
any vault on the machine. Nothing on the consent path saw it: the carrier server declared
no `secrets`, so there was nothing to fingerprint and nothing to prompt about (#453). A
name this refuses that your host would have accepted costs you a rename in one committed
file; the other direction cost the vaults.

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
  reddit/acme → type "http"  url "https://api.acme.example/mcp"  env "HTTPS_PROXY"="http://p.example:3128"  secrets "ACME_TOKEN"="acme-token"  vault "reddit"
    approve reddit/acme? [y/N]
```

Anything but an explicit yes withholds, and declining a server that was approved before
revokes it. `--dry-run` shows the same lines and records nothing. `--yes` approves every
credentialed server without asking, and is **required** off a terminal — a flag that means
yes where nobody can be asked is not consent.

**What is recorded is the line itself.** The fingerprint is the SHA-256 of the text
printed above the question and nothing else is mixed into it, which makes two properties
true at once and by construction: two entries that print the same line share one approval,
and an entry that prints a different line lapses it. There is no second, shorter summary
that can fall out of step with what you read — three earlier rounds of this feature each
had one, and every bypass since has been one field that was in the digest and not on the
line, which meant being re-asked under a line byte-identical to the one already approved.

That puts the weight on the line holding **everything**. It does. The renderer loops over
the entry's keys rather than over a list of fields charter knows, so:

* `command` and `args` print after `run`, one word per word — quoted when a word is empty
  or contains a space, so where the words split is never in doubt;
* `type`, `url`, `env`, `secrets` and `secret_files` print under their own names;
* **`env` prints its values, not only its keys.** The value is the half that decides:
  `PATH` chooses which binary `execvpe` finds, `NODE_OPTIONS` chooses what it loads. An
  `env` value is committed plaintext out of `mcp.json` — not a vault value — so printing it
  discloses nothing that reading the repo would not;
* **the vault is named**, because `vault:` is a key of the committed `persona.md` and a
  one-line commit there re-points which credential is spent;
* **any key charter has never been taught** — `cwd`, `headers`, whatever comes next —
  prints under its own quoted name with its JSON value, because `sync-agents` passes every
  key it does not consume through to the harness.

Charter's own words are printed bare and everything committed is printed between quotes,
so a committed value cannot dress itself up as part of charter's sentence.

**Which credential, and not only which command.** `secrets` and `secret_files` map an
environment variable to a *vault key*, and that key decides which of the vault's values the
command receives — so both are on the line as `"VAR"="key"`, in the shape the `secret exec`
argv is built from. The credential's **value** is the one thing that is not on the line and
cannot be: it is not in the entry, and the process that prints this never opens a vault.

**The line is printable ASCII, and everything else is spelled out as `\uXXXX`.** Not
"unprintable characters are escaped" — every codepoint outside `U+0020..U+007E` is,
whatever its category. A committed `args` can otherwise carry a `\r` that repaints the
line, a bidi override that reverses it, a combining mark that repaints the rows around it,
a U+3164 HANGUL FILLER that is printable and renders as nothing, or a Cyrillic `а` that
makes `api.асme.example` indistinguishable from `api.acme.example` on the one line the
decision rests on. MCP commands, args, urls and env keys are ASCII in practice, so
anything else here is a reason to *show the escape* rather than the glyph. Escaping is
also what makes "renders as nothing" answerable: charter decides it on the escaped line,
where the ASCII space is the only character left that shows nothing.

Read that claim precisely, because it is the kind of sentence this section has already had
to withdraw twice: *everything on the row that came out of a committed file* is printable
ASCII. The `•` and the `→` around it are charter's own punctuation, put there by charter
and not by anyone's `mcp.json`. The test derives that set from a benign run rather than
listing it, with colour pinned off so the derivation cannot quietly absorb an escape
sequence the environment happened to add.

The escaping is **reversible**, which is the part that makes reading the line worth
anything — and, now that the fingerprint is taken over the line, the part that makes the
approval mean the entry. Astral codepoints use the eight-digit `\UXXXXXXXX` form
(`\u1f600` is five hex digits and would also spell `U+1F60` followed by `0`); a literal
backslash is doubled and a literal quote is `\"`, so every `\uXXXX` you see is a codepoint
that was really there rather than six ASCII characters imitating one, and an unescaped
quote is always charter's own delimiter. A Windows path therefore shows as `C:\\Users\\x`.
Nothing is collapsed, stripped or shortened on the way: a run of spaces prints as a run of
spaces and costs the columns it occupies, because a part that got tidied away is a part you
did not consent to.

That covers the **whole** line, including the `persona/server` label in front of the
arrow — the half that had gone to the terminal untouched while the destination beside it
was hardened three times. A server name is a key of a committed `mcp.json`: an arbitrary
string, of arbitrary length, in any script. So a server named with three U+3164 fillers no
longer prints as `reddit/ → uvx`, one carrying an ANSI erase no longer wipes the words
standing beside it, and one of a hundred thousand characters no longer puts twelve hundred
rows in front of the destination. Both halves of the label are clipped to a fixed width
and escaped the same way, and the destination's own budget is what is left of the screen
once the label has been paid for — a ceiling on the part charter was looking at, rather
than on the line it prints, is a ceiling the other part is free to walk past.

(A persona name cannot reach that line hostile in the first place: `personas/` entries are
held to `[a-z0-9][a-z0-9._-]*`. It goes through the same escape anyway — charter joins its
guards rather than choosing between them — and the clip is not belt-and-braces at all,
since that alphabet bounds the characters and not the length.)

An entry charter cannot show in full cannot be approved at all and is reported as withheld.
Two ways to get there: it names no destination (no `command`, no `args`, no `url` — a part
that renders as nothing does not count as naming something), or its full rendering **would
not fit on one screen**. That ceiling is a screen and not a byte count on purpose: you
answer the prompt printed *under* the line, so a line taller than the terminal has already
scrolled the command it names off the top before the question reaches you. Nothing is ever
trimmed to fit it — an earlier round clipped each part at two hundred characters and
announced the cut, which bounded the line but let two different `args` print the same tail.
Complete, or refused.

**The scope, said plainly.** This is a guard against a *commit* — a file changing under an
approval you already gave — answered by a person reading one line. It is not a guard
against someone who can already run code as you: they can edit the approval record, the
harness, or charter itself. `SECURITY.md` states charter's boundary and nothing on this
page exceeds it.

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
