---
version: unreleased
headline: An MCP approval is a digest of the line you read, and is now asked rather than assumed
adopt: persona sync-agents --approve-mcp
---

`charter persona sync-agents --approve-mcp` is the consent that lets a committed `mcp.json`
hand a persona's vault value to the command it names. Three findings from the 2026-08-24
audit each hollowed it out from a different side, and four rounds of review then found the
same class of hole in the fixes themselves — every round closed the instance it was shown,
and every round an attacker found the next one. All of it is fixed, and **every existing
approval lapses** — see the last section.

**The record is now the line you read, and nothing else.** Rounds one to three kept two
representations of the entry: a digest over all of it, and a printed line built from a list
of seven fields. Every bypass since was one field that lived in the first and not the
second — the persona's vault, `env` VALUES, `cwd`, a clipped tail — so the approval
correctly lapsed, you were correctly asked again, and the line you were asked under was
byte-identical to the one you had already approved. That is not consent; it is a second
chance to make the same mistake. The fingerprint is now the SHA-256 of the printed line, so
*two entries that print the same line share one approval* holds by construction, and the
whole question becomes whether the line holds everything — which is now decided by a loop
over the entry's keys instead of by a list of fields.

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
field past #330. Consent now covers the **entire entry**: every key of it is printed on the
line, and the line is what is hashed, so a key charter has not been taught about yet cannot
fall outside either. Key order still does not matter; a re-serialised file will not nag
you.

**An `http` server's consent line was blank.** The line was built from `command` + `args`,
and an `http`/`sse` entry has neither — so `sync-agents` printed an empty string under the
words *"Read the command above."* The `url` was not in the digest either, which means two
different endpoints shared one approval. Both are now covered: the line names the `url` and
the `type`, and escapes anything outside printable ASCII (see below),
because a `\r` or a bidi override in a committed `args` can otherwise repaint the one line
the whole decision rests on. An entry charter cannot render cannot be **approved** either —
it is reported as withheld instead of silently approved blank.

**And nothing on that line can push anything else off it.** The first cut at this clipped
the finished line at 600 characters, on the reasoning that the destination sits at the
front. That was true only of `command`. Both the `[type url]` and the `(env: …)` parts are
appended *after* `args`, so roughly 600 characters of plausible-looking `args` in a
committed file produced a consent line naming neither the `PATH` it re-pointed nor the
endpoint it connected to — while the entry the operator approved carried both through to
`execvpe`. The second cut gave each part its own 200-character budget and announced the
cut, which bounded the line but was not one-to-one: two `--config` payloads agreeing on
their first 200 characters and equal in length printed the same text and the same
`(+N more chars)` count.

So nothing is shortened at all any more. The line is **complete or there is no line** — an
entry whose full rendering does not fit on the screen the question is asked on is refused
and reported as withheld, exactly as an unrenderable one always was. A padded part cannot
shorten another part, because no part is ever shortened.

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
  reddit/reddit → run \u3164\u3164\u3164  type "stdio"  secrets "ACME_TOKEN"="acme-token"  vault "reddit"
  reddit/acme   → type "http"  url "https://api.\u0430\u0441me.example/mcp"  …
```

Precisely: *everything on that row which came out of a committed file* is printable
ASCII. The `•` and the `→` around it are charter's own punctuation. (The test that pins
this derives charter's own glyphs from a benign run — with colour pinned off, because
`util` decides colour at import time from `stderr.isatty()`, and running the suite from a
terminal used to fold charter's own escape sequences into the derived set and make the
assertion unfailable for exactly the class it exists to catch.)

That closes three shapes at once. Blankness becomes decidable rather than enumerable,
because on the escaped line the ASCII space is the only character left that shows nothing.
A combining mark can no longer repaint the rows around the line. And a homoglyph re-point
becomes *readable*: `api.\u0430\u0441me.example` already lapsed the approval and re-asked you — the
url is in the digest — but the old line was pixel-identical to `api.acme.example`, so
being asked again told you nothing.

The escape is **reversible**, or it would just move the problem: astral codepoints use the
eight-digit `\UXXXXXXXX` form, since `\u1f600` is five hex digits and would equally spell
`U+1F60` followed by `0`; a literal backslash is doubled and a literal quote becomes `\"`,
so a committed `command` holding the six ASCII characters `\u3164` cannot imitate one
holding U+3164, and an unescaped quote is always charter's own delimiter. Windows paths
show as `C:\\Users\\x`. Nothing is collapsed or stripped either: an earlier round tidied
runs of ASCII spaces out of every part, so `"   --evil"` printed as `--evil` — a line that
no longer said what would run.

And it covers the whole line, not the destination half of it. The `persona/server` label
printed in front of the arrow had gone to the terminal untouched while the destination
beside it was hardened three times over — which is this fix's own lesson arriving at its
own expense. A server name is a key of a committed `mcp.json`: an arbitrary string, of
arbitrary length, in any script. Three fillers printed as `reddit/ → uvx`; an ANSI erase
wiped charter's own words standing beside it; a hundred thousand characters put twelve
hundred rows in front of the destination while the line's own ceiling was satisfied,
because the function that enforced it never saw the name.

Both halves of the label are now escaped and clipped to a fixed width, and the
destination's ceiling is what is left of the screen once the label is paid for. A ceiling
on the part you were looking at, rather than on the line you print, is a ceiling the other
part walks past. (A persona name cannot be hostile to begin with — `personas/` entries are
held to `[a-z0-9][a-z0-9._-]*` — but that alphabet bounds the characters and not the
length, and it goes through the same escape regardless, because charter joins its guards
rather than choosing between them.)

The *other* row of the same report gets the same escape. 0.52.0 bounded MCP server names at
the boundary that reads them and prints a `Refused …` list of the names it turned away —
and printed those through `contain.one_line`, which answers a different question. That one
guarantees a value cannot forge a second row, by escaping the categories that carry no
glyph (`Cc`, `Cf`, `Cs`, `Zl`, `Zp`); it says in its own docstring that it does not make a
value readable. U+3164 HANGUL FILLER is `Lo` and U+2800 BRAILLE PATTERN BLANK is `So`, so a
refused name made of them printed `acme/` with nothing after the slash — telling you to go
rename a server you cannot see the name of, one row above a withheld row that had already
been hardened against exactly that. Both rows now use the same escape, and so does
`charter persona lint`.

**And the line says which credential, not only which command.** `secrets` and
`secret_files` map an environment variable to a *vault key*, and that key is what decides
which of the vault's values the command receives. They were in the digest and not on the
line, so editing `{"REDDIT_CLIENT_ID": "client-id"}` to `{"REDDIT_CLIENT_ID": "aws-root-key"}`
lapsed the approval, asked you again, and asked under a line byte-for-byte identical to the
one you had already approved — the same shape as the homoglyph above, in the field that
chooses the credential rather than the destination. Both now print as `"VAR"="key"`, the
shape the `secret exec` argv is built from. The credential's **value** is the one thing not
on the line and it cannot be: it is not in the entry, and the process that prints this
never opens a vault.

**And which vault, whose env, and every key charter has never been taught.** Three more of
the same shape, closed together rather than one at a time. `vault:` is a key of the
committed `persona.md`, so a one-line commit re-points which credential is spent — it was
digested from the first commit and printed by none, so even the *first* prompt could not
say whose credential was at stake, while the line printed charter's own word `(vault: …)`
in front of the variable name. `env` printed its KEYS while the VALUE is the half that
decides — `PATH` chooses which binary `execvpe` finds. And a key charter does not read at
all (`cwd`, `headers`, whatever comes next) was passed through to the harness and never
shown. All three are on the line now, and the last of them because the renderer loops over
the entry's keys rather than over a list of fields:

```
  reddit/reddit → run uvx some-reddit-mcp  type "stdio"  env "PATH"="/usr/bin"  secrets "REDDIT_CLIENT_ID"="client-id"  "cwd" "/home/me/proj"  vault "reddit"
```

Charter's own words print bare and everything committed prints between quotes, so a
committed value cannot dress itself up as part of charter's sentence.

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
  reddit/acme → type "http"  url "https://api.acme.example/mcp"  env "HTTPS_PROXY"="http://p.example:3128"  secrets "ACME_TOKEN"="acme-token"  vault "reddit"
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
time what gets recorded is a digest of the line in front of you.

**And the scope, said plainly**, because a local page promising more than `SECURITY.md`
does is itself a defect. This is a guard against a *commit* — a committed file changing
under an approval you already gave — answered by a person reading one line. It is not a
guard against someone who can already run code as you: they can edit the approval record,
the harness, or charter itself.
