---
version: unreleased
headline: An MCP approval covers the whole entry, and is now asked rather than assumed
adopt: persona sync-agents --approve-mcp
---

`charter persona sync-agents --approve-mcp` is the consent that lets a committed `mcp.json`
hand a persona's vault value to the command it names. Three findings from the 2026-08-24
audit each hollowed it out from a different side, and two rounds of review then found the
same class of hole in the fixes themselves. All of it is fixed, and **every existing
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
URL, shows the `env` keys, and escapes anything outside printable ASCII (see below),
because a `\r` or a bidi override in a committed `args` can otherwise repaint the one line
the whole decision rests on. An entry charter cannot render cannot be **approved** either —
it is reported as withheld instead of silently approved blank.

**And nothing on that line can push anything else off it.** The first cut at this clipped
the finished line at 600 characters, on the reasoning that the destination sits at the
front. That was true only of `command`. Both the `[type url]` and the `(env: …)` parts are
appended *after* `args`, so roughly 600 characters of plausible-looking `args` in a
committed file produced a consent line naming neither the `PATH` it re-pointed nor the
endpoint it connected to — while the entry the operator approved carried both through to
`execvpe`. Every part now has its own budget and is clipped on its own, with the cut
announced. `args` can be a megabyte long and the line still *ends* with the endpoint and
the `env` keys it used to push off — here is the tail of one whose `--config` value is 643
characters:

```
  … (+443 more chars)  [stdio https://evil.example/mcp]  (env: NODE_OPTIONS, PATH)
```

**The consent line is printable ASCII, and everything else is spelled out.** The first two
attempts at this guard matched something narrower and were walked past by the same attack
in a new spelling each time. One escaped only what `str.isprintable` rejects and stripped
runs of the ASCII space — which leaves U+3164 HANGUL FILLER, U+2800 BRAILLE PATTERN BLANK
and U+115F/U+1160 straight through: all printable, none whitespace, all `strip`-proof, all
blank on every terminal. A `command` of three of them rendered a line blank to the reader
and truthy to charter, restoring the very blank approval this section calls impossible.
There is no list of codepoints that ends, so the rule is now the complement — `U+0020` to
`U+007E` is what a consent line may contain, and every other codepoint, in any category
and any plane, is shown as `\uXXXX`:

```
  reddit/reddit → \u3164\u3164\u3164
  reddit/acme   → http https://api.\u0430\u0441me.example/mcp
```

That closes three shapes at once. Blankness becomes decidable rather than enumerable,
because on the escaped line the ASCII space is the only character left that shows nothing.
A combining mark can no longer repaint the rows around the line. And a homoglyph re-point
becomes *readable*: `api.асme.example` already lapsed the approval and re-asked you — the
url is in the digest — but the old line was pixel-identical to `api.acme.example`, so
being asked again told you nothing.

The escape is one-to-one, or it would just move the problem: astral codepoints use the
eight-digit `\UXXXXXXXX` form, since `\u1f600` is five hex digits and would equally spell
`U+1F60` followed by `0`, and a literal backslash is doubled, so a committed `command`
holding the six ASCII characters `\u3164` cannot imitate one holding U+3164. Windows paths
show as `C:\\Users\\x`.

**And the line has to fit on a screen.** The ceiling on the whole line used to be 2000
characters — twenty-five rows of an 80-column terminal. Nine args of 200 invisible columns
each fit under it as one 1837-character line: you saw `uvx evil-server`, twenty-two blank
rows, then `(env: PATH)`, and by the time the prompt was printed the command had scrolled
off the top. Escaping makes that padding visible, which is necessary and not sufficient —
visible padding scrolls a line just as far. The ceiling is now the screen the question is
asked on, ten rows of eighty columns, and an entry that overflows it is refused rather than
cut in half: charter will not decide which half of a destination you get to read. Real
entries are nowhere near it; a `docker run` server with three `env` keys is under 250
characters.

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
restored. The refusal you get off a terminal does not name `--yes` — `--help` does. A
refusal that prints the flag defeating it is #421's shape, and the reader of that line is
as often an agent as a person.

**What to do after upgrading.** The digest changed, so every fingerprint recorded on this
machine is stale and every credentialed MCP server is withheld until you approve it again.
Nothing breaks loudly — that is the withholding design, and it is the failure direction you
want — but a server will fail to authenticate rather than start, and `sync-agents` will name
each one it withheld from. Run `charter persona sync-agents --approve-mcp`, read the lines,
and answer. You are re-reading commands you already read once; the difference is that this
time the answer covers where the credential goes, not five fields of it.
