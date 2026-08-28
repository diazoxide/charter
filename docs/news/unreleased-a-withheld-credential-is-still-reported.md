---
version: unreleased
headline: `persona lint` and `doctor` report a persona running without the credential it declares, instead of it being said once and lost
security: true
---

When `charter persona sync-agents` withholds a vault from an MCP server, it says so, and it
names the command that would restore it. That warning is correct and it was the **only**
place it was ever said ([#489](https://github.com/diazoxide/charter/issues/489)).

`persona.mcp_withheld` had exactly one caller — `sync-agents`, on the run that wrote the
file. Once the terminal scrolled, a persona running without its credential was
indistinguishable from one that never declared one:

```
$ cat .claude/agents/ws.md | grep ga4
  - {"ga4": {"command": "uvx", "args": ["analytics-mcp==0.7.0"]}}
$ charter persona lint          # nothing
$ charter doctor                # nothing
```

That rendering is **byte-identical** to what an entry with no `secrets` at all produces —
in a generated file people are told never to hand-edit, so it reads as intended output.
The failure it produces arrives three layers away, as an MCP server that will not
authenticate. This matters most in exactly the transition it was reported from: a plane
upgrading past the consent gate rewrites every credentialed server in every persona on the
first `sync-agents`, while the operator's attention is on whatever they actually ran the
command for.

**`charter persona lint` now names each one**, with both ways out — approve the command, or
drop the `secrets`/`secret_files` declaration if the server should hold no credential.
`charter doctor` reaches it through the personas line, which already runs `lint` across the
roster; a `doctor` row of its own would be a second sentence about one fact, and two
sentences about one fact is how the pair comes to disagree.

**A warning, not an error, and that is a decision.** Withholding is the gate *working*: you
may have read the command and declined it, and a lint that exits 1 forever is a lint planes
turn off. What charter owes is that the state stays visible, not that it overrules your
answer. The one case that *is* an error is an entry charter cannot show in full — no
approval can ever exist for it, `--approve-mcp` refuses it by name, so "approve it" is
advice that cannot work and the committed entry has to change.

**The generated agent deliberately gains no comment of its own**, which the issue floats.
The consent record has learned across four rounds that this class closes by *not* keeping a
second representation — the fingerprint is the SHA-256 of the line itself precisely so
there is nothing left to fall out of step with. A comment in a wholesale-regenerated file is
exactly a second representation: it goes stale the moment you approve without re-syncing,
and a reader who trusts it then reads a false statement about a live credential. The file is
where the question gets asked; the record is where it is answered, and `lint` asks the
record.

**Two siblings, found by enumerating the surfaces rather than by being filed.**

`lint`'s "declares `secrets` but names no vault" error read `secrets` and not
`secret_files`, while `mcpseen.needs_consent`, `mcp_render_entry` and `charter secret exec`'s
own `--env`/`--file` pair all treat the two as one mechanism. So a server declaring only
`secret_files` against a persona with no vault rendered with no credential and was reported
by **nothing**: not by that error, because the key was not read, and not as withheld,
because with no vault there is nothing to withhold. `secret_files` is not the exotic half —
it is what Google ADC needs, and it is the declaration the issue's own reproduction carries.
Both keys now go through one function.

And `vault: none` — charter's reserved way of saying *this persona deliberately holds no
credentials* — was read as the name of a vault by the MCP path, though `persona.vault_of`
has always returned `None` for it. Measured against 0.53.0:

```
consent line   run uvx analytics-mcp  secrets "T"="k"  vault "none"
rendered       charter secret exec none --env T=k --exec -- uvx analytics-mcp
```

So `--approve-mcp` asked you to approve spending a vault named `none`, recorded the
consent, and wrote a launcher into the generated agent that cannot run — while `lint` was
separately and correctly calling the same persona one that names no vault. One sentinel,
two readings, and the consent record was on the wrong side of it. The render and the consent
list now normalise it in one place, because those two disagreeing is an operator asked about
a server the file does not wrap.
