---
version: unreleased
headline: The vault documentation now says what the guard actually catches, and what walks past it
---

charter's central claim about a vault has always been *"the model names the secret; it
never sees it."* The half of that sentence charter can keep is real and unchanged: the
value is resolved inside charter's own process and never returned to a caller as a value.
The half nobody had written down is that **the model chooses the command charter runs**,
and charter's guards see a program name and a path spelling — not a property.

A security review pointed four independent rounds of attack at those guards. Every added
parser lost to a new spelling and one attempt shipped a regression, so this release closes
none of them in code. It writes them down instead, in the places that were claiming more
than the code delivers.

**The worst line was the one the model reads.** `skills/secrets/SKILL.md` told the agent
that a secret is *"redacted from its output, so a command that echoes it still cannot leak
it into the transcript."* Redaction is `str.replace` over the bytes that came back. It
masks a `curl -v` that echoes an `Authorization` header — the accident it was built for —
and it cannot touch a value the command *transformed*: `printf %s "$T" | base64` returns
the credential in full through the ordinary, redacting path. `--exec` and `--stream`
capture nothing at all, and so redact nothing, and that file never mentioned them. It now
says net, not boundary, and adds two hard rules an agent can act on: you choose the
command and charter trusts your choice, and `secret cp` hands a *path* to a tool that needs
one — it is not a way to get at the value.

**`SECURITY.md`, `README.md` and `docs/hooks.md` carried the same overclaim in weaker
form** — the mermaid note reading *"no step here ever put the value in a context window"*
sat directly under a step in which the model picks the command. Each now separates the
guarantee (*charter never prints the value into the conversation*) from what depends on the
command you chose. `SECURITY.md` gains the shape of the guard's ceiling in three bullets:
it matches program names, so `python3 -c`, `base64`, `cp` and `git show HEAD:<path>` run;
it reads the argv it is handed, so `sh -c 'cat …'` is one opaque argument; and it matches a
path spelling, so a vault registered outside `.charter/` and a file `secret cp` wrote where
you asked are ordinary files to it. Widening the name list is not the fix and is not
planned — the missing name is always the next one, and a guard that denies real work gets
switched off.

**The denial texts stopped pointing at the door.** Refused `--reveal`, an agent read *"Use
`charter … secret exec`/`cp`"* — and `secret cp <vault> <key> /tmp/x` followed by `cat
/tmp/x` is two allowed commands to the same bytes. Both leak denials, and the `Read`/`Grep`
one, now name `secret exec` as the route that keeps the value out of the conversation and
say plainly what `cp` is for and that reading a materialised copy back is the same leak by
another road. `charter secret get`'s own masked-output hint and its non-interactive
`--reveal` refusal say the same. The first 70 characters of each denial are unchanged,
because that prefix is the tally key `charter guard` counts by.

**One thing did change in code, because writing the limit down exposed it.** The sentence
"a known reader pointed at a path under `.charter/`" turned out to be false for a path
spelled with a redundant separator: `cat .charter/vaults/db.json` was denied and
`cat .charter//vaults/db.json` was allowed — the same file, the same program, one keystroke
apart, and nothing exotic about it. Both guards now canonicalise the operand before
matching, so `//`, `/./` and `a/b/..` collapse to the one spelling. The trailing slash is
put back afterwards, because `grep -r . .charter/vaults/` names the directory that holds
every vault file and must stay denied. This is not a resolver: a symlink you planted, and a
path outside the plane, are still the documented limit, because following them would mean a
`stat` on every operand of every command.

**Two new test files pin all of it.** `tests/test_documented_limits.py` asserts the limits
as current behaviour, each class with a positive control, so the day someone narrows or
widens one of these guards the suite points at the paragraph that has to move with it.
`tests/test_vault_path_spellings.py` generates equivalent spellings of one path rather than
listing bad strings, and asserts every one of them lands where the canonical form lands —
denied for a vault file, allowed for the registry beside it.
