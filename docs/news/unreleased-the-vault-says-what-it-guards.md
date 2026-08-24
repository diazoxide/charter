---
version: unreleased
headline: The vault documentation now says what the guard actually catches, and what walks past it
---

charter's central claim about a vault has always been *"the model names the secret; it
never sees it."* The half of that sentence charter can keep is real and unchanged: the
value is resolved inside charter's own process and never returned to a caller as a value.
The half nobody had written down is that **the model chooses the command charter runs**,
and charter's guards see a program name and a path spelling — not a property.

A security review pointed five independent rounds of attack at those guards. Almost every
added parser lost to a new spelling and one attempt shipped a regression, so this release
closes in code only what a guard of this shape can be *complete* over — the text of an
operand as written — and writes the rest down, in the places that were claiming more than
the code delivers. The rule the review kept proving, and the one this entry is written to
obey: **where the code cannot make a sentence true, the sentence changes.**

**The worst line was the one the model reads.** `skills/secrets/SKILL.md` told the agent
that a secret is *"redacted from its output, so a command that echoes it still cannot leak
it into the transcript."* Redaction is `str.replace` over the bytes that came back. It
masks a `curl -v` that echoes an `Authorization` header — the accident it was built for —
and it cannot touch a value the command *transformed*: `printf %s "$T" | base64` returns
the credential in full through the ordinary, redacting path. `--exec` and `--stream`
capture nothing at all, and so redact nothing, and that file never mentioned them. It now
says net, not boundary, and adds three hard rules an agent can act on: you choose the
command and charter trusts your choice; `secret cp` hands a *path* to a tool that needs one
and is not a way to get at the value; and *"the guard allowed it"* is not evidence a command
is safe, so an allowed spelling of a refused read is still a refused read.

**`SECURITY.md`, `README.md` and `docs/hooks.md` carried the same overclaim in weaker
form** — the mermaid note reading *"no step here ever put the value in a context window"*
sat directly under a step in which the model picks the command. Each now separates the
guarantee (*charter never prints the value into the conversation*) from what depends on the
command you chose. `SECURITY.md` gains the shape of the guard's ceiling in bullets: it
matches program names, so `python3 -c`, `base64`, `cp` and `git show HEAD:<path>` run; it
reads the argv it is handed, so `sh -c 'cat …'` is one opaque argument; it matches a path
spelling, so a vault registered outside `.charter/` and a file `secret cp` wrote where you
asked are ordinary files to it; and it runs before any shell, so a glob, a `$VAR` or a `cd`
reaches the same file it refuses when the path is spelled plainly. Widening the name list is
not the fix and is not planned — the missing name is always the next one, and a guard that
denies real work gets switched off.

**The denial texts stopped pointing at the door.** Refused `--reveal`, an agent read *"Use
`charter … secret exec`/`cp`"* — and `secret cp <vault> <key> /tmp/x` followed by `cat
/tmp/x` is two allowed commands to the same bytes. Both leak denials, and the `Read`/`Grep`
one, now name `secret exec` as the route that keeps the value out of the conversation and
say plainly what `cp` is for and that reading a materialised copy back is the same leak by
another road. `charter secret get`'s own masked-output hint and its non-interactive
`--reveal` refusal say the same. The first 70 characters of each denial are unchanged,
because that prefix is the tally key `charter guard` counts by.

**Three things did change in code, because writing the limit down exposed them.** The
sentence "a known reader pointed at a path under `.charter/`" turned out to be false for a
path spelled with a redundant separator: `cat .charter/vaults/db.json` was denied and
`cat .charter//vaults/db.json` was allowed — the same file, the same program, one keystroke
apart. It was false again for letter case: on macOS and Windows the filesystem folds case,
so `cat .CHARTER/vaults/db.json` and `cat .charter/VAULTS/db.json` were the same inode as
the denied form, and the guard — which already lower-cased the *program* name — was allowing
both, on the platform this project is developed on. Both guards now normalise the operand
before matching: `//`, `/./` and `a/b/..` collapse to one spelling, and the match is
case-insensitive.

It was false a third time for the one operand that walks EVERY vault file at once. The
pattern demanded a literal `.charter/vaults/`, and the `Read`/`Grep` guard had quietly
papered over that by retrying each target with a `/` appended — a step that lived in that
one caller. So `Grep(path=".charter/vaults")` was refused while `grep -rn TOKEN
.charter/vaults` printed a password, along with every slash-less respelling of the same
directory: `.charter//vaults`, `.charter/./vaults`, `./.charter/vaults`, `.CHARTER/vaults`,
`.charter/x/../vaults`, `.edm/vaults`. One predicate, two answers, and the gap sat exactly
where `pretooluse_read`'s own docstring said a gap between the two guards would sit. The
fix is in the pattern, not in a caller: `vaults` is now anchored to a path SEGMENT — a `/`
or the end of the operand — which answers the directory on both routes for the same reason
and keeps `.charter/vaults.json`, the registry, an ordinary read. The caller's retry is
gone, and so is a second slash-restoring step the anchor made dead. What replaces both is a
test that asserts the *property* rather than either guard's own list: for a generated corpus
of spellings, the Bash route and the `Read`/`Grep` route must return the same answer, in
both directions.

None of the three is a resolver: a symlink you planted, and a path outside the plane, are
still the documented limit, because following them would mean a `stat` on every operand of
every command.

**What that closes is a class of SPELLINGS, and the first draft of this entry overstated
which one.** It said the operand is canonicalised "so `//`, `/./` and `a/b/..` collapse to
the one spelling", presented as closing the class *the same file, the same program, one
keystroke apart*. That class is not closed and cannot be by a guard of this shape.
`cat .charter/vault?/db.json` is the same file, the same program, one keystroke apart, and
allowed — because the shell expands the glob after the hook has already answered, on text
the hook never sees. So are `head -c 400 .charter/vault*/db.json`,
`V=.charter/vaults/db.json; cat $V` and `cd .charter/vaults && cat db.json`. What the guard
can be complete over is narrower than the first draft claimed and narrower than the second
draft claimed: the text of an operand as written, over redundant `/` separators, dot
segments, letter case, and the presence or absence of a trailing slash on the vault
directory. That is now true on both routes and asserted to be the same on both. It is
still not everything a reader might mean by "separators" or by "the path it names", and
two known cases are filed rather than implied:

* **#474** — an operand that *contains* the vault directory without naming it.
  `grep -rn TOKEN .` from the plane root reads every vault file and is allowed, on both
  routes, deliberately: denying every broad search is untenable. `charter init` gitignores
  the whole of `/.charter/` for the same class of reason, and `skills/secrets/SKILL.md` now
  tells the agent to exclude the directory rather than rely on the guard.
* **#476** — a Windows-style `.charter\vaults\db.json` is not folded, because on POSIX a
  backslash is an ordinary filename character and folding it would deny real filenames. The
  docs say "`/` separators" instead of "separators" for exactly this reason.

Both are pinned as current behaviour in `tests/test_vault_path_spellings.py`, so the day
either changes the test points at the paragraph that has to move with it.

What a *shell* does to that text afterwards is a sixth
documented limit, stated in that form in `SECURITY.md`, `docs/hooks.md`, `docs/secrets.md`
and `skills/secrets/SKILL.md`, and pinned as behaviour — each case proved to really read the
file, not merely asserted to be allowed — in `tests/test_documented_limits.py`. Reaching for
it construct by construct would put a shell inside the guard, and the hole would then be
shaped like whichever construct that shell got wrong.

**Two new test files pin all of it.** `tests/test_documented_limits.py` asserts the limits
as current behaviour, each class with a positive control, so the day someone narrows or
widens one of these guards the suite points at the paragraph that has to move with it.
`tests/test_vault_path_spellings.py` generates equivalent spellings of one path rather than
listing bad strings — including all 8192 upper/lower spellings of `.charter/vaults/`, not a
sample of them — and asserts every one lands where the canonical form lands: denied for a
vault file, allowed for the registry beside it. It also carries two differentials. One is against
`origin/main`'s predicate, transcribed and frozen, asserting that nothing main denied is
allowed here; a security change that denies less than the branch it came from is a
regression wearing a fix's commit message, and this review round produced two of those. The
other is between charter's own two guards, and it is the one that would have caught the
directory bypass: every other test in that file asks a guard about the operands it was
written for, which is why both files were green while one route denied what the other
printed.
