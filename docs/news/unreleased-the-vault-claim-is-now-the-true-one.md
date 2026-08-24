---
headline: The vault's headline claim now says what charter guarantees, which is narrower than what it said
---

charter's front page said *"the model never sees the value"*. `skills/secrets/SKILL.md` —
the page loaded into the model's own context — said it harder: *"In every case the value is
injected into the subprocess and redacted from its output, so a command that echoes it
still cannot leak it into the transcript."*

Both are false, and not by a clever exploit. Redaction is a string replacement over the
output charter captured, so a command that *transforms* the value defeats it through the
ordinary path:

```
charter secret exec demo --env T=T -- sh -c 'echo "$T"'            # ***
charter secret exec demo --env T=T -- sh -c 'printf %s "$T" | base64'   # the value, in base64
```

`--exec` and `--stream` capture nothing at all and therefore redact nothing, which
`docs/secrets.md` has always said and `charter/secrets/base.py` has always called *"a
defence-in-depth net."* The defect was never the mechanism. It was that **the strength of
the claim rose as the reader's ability to check it fell**: correct on the page you reach
after going looking, unqualified on the front page, and strongest of all in the file read
by the one reader who cannot go and check.

**The guarantee, stated the way it is actually kept.** *On the paths that consume a value
— `secret exec` with `--env`/`--file`/`--dotenv`, and the MCP launcher — charter never
prints the value into the conversation, and everywhere else charter prints it only into a
destination you named yourself.* Where the value goes after that is a property of the
command you asked charter to run. Read `charter secret exec <vault> -- <cmd>` with the same
suspicion you would read `<cmd>` holding the credential directly, because that is what it
is.

That sentence, and the paragraph that bounds it, stand in the four pages that state the
guarantee: `SECURITY.md`, the README, `docs/secrets.md` and `docs/mcp.md`. The other four
pages that touch the vault — `docs/hooks.md`, `docs/git-policy.md` and the two model-facing
skills — describe a guard or a lane rather than the guarantee, and what they carry is the
bound relevant to *them*, checked by `TestEveryClaimSurfaceCarriesTheLimit`. An earlier
draft of this entry said the sentence stood in all eight. It stood in four, which is the
same defect as the one this whole issue is about, in the release note announcing the fix.

**The first draft of that sentence was itself false, which is the whole point of the
issue.** It read *"charter never prints the value into the conversation"*, full stop — and a
reviewer whose only instruction was to break it ran three words of charter's own CLI:

```
$ charter secret cp demo TOKEN /dev/stdout
FAKE-NOT-A-REAL-SECRET-9931✓ Wrote 'demo/TOKEN' to /dev/stdout (0600). Value not shown.
```

That is charter's own process writing the credential into the transcript, with no child
command anywhere in it, and then reporting that it did not. `charter secret get --reveal
--force` is the second route. The `cp` half has since been closed by
[#449](https://github.com/diazoxide/charter/pull/449) — the destination is now refused by
**identity** rather than by name, `(st_dev, st_ino)` from an `fstat` of the descriptor
charter opened against its own three streams, so `/dev/stdout`, `/dev/fd/1`,
`/proc/self/fd/1`, the transcript's real path and any hardlink to it get one answer, and
`--force` does not reach the check. `get --reveal` remains the one path where charter's
own process writes a plaintext value to its own stdout; it refuses a non-interactive
stdout unless you pass `--force`, and `--force` is a real override.

The sentence is bounded here anyway, and that is the point rather than an accident of
timing: it was written while both routes were open, so it never depended on a fix that had
not landed. A guarantee whose truth is scheduled is not a guarantee.

**The model gets two new hard rules**, because it is the reader that acts on this file
rather than filing an issue about it: never pass a secret to a command whose recipient you
did not choose, and never `secret cp` to anything but a real file path you named.
`/dev/stdout` and friends put the value straight into this conversation.

**Two guard descriptions stopped overstating their own reach.** `docs/hooks.md` described
the secret-leak guard as catching "a command whose argv would put a vault's contents into
the transcript", which is a semantic property no name-based check has; `docs/secrets.md`
said the hook "denies `--reveal` outright". Both now describe what the guard is: a check on
names in the argv it can see, which closes the accidental roads and not a chosen one.

**The prose test stopped exempting anything that says "charter".** Round one asked whether
the word appeared anywhere in the sentence, and the same reviewer rescued four of this
file's own historical claims by pasting it into a subordinate clause — *"Because charter
resolves it in its own process, the model never sees the value"* passed. A guard against
overclaiming that is silenced by mentioning the project's name is not a guard. Now the
question is positional and asked of the clause the absolute word sits in: the bound has to
be in the *same sentence*, naming charter is necessary and never sufficient, and a promise
whose subject is the model or the value itself fails whatever else the sentence says. The
verb lists grew for the same reason — "the value is **stripped** from every output" and
"the value never **enters** an agent's context" were both synonyms away from the words the
first draft knew.

**The prose test was handed to two more reviewers, and it fell again.** Six more classes
got past round two, and each was the same mistake in a new spelling: the rule read
backwards but not forwards (round one's bypass came back verbatim with the clauses
swapped); it kept two word lists for the one idea "a place a reader sees text", and one of
them knew `chat`; it wrote the open class of credential nouns as five words, so *"Your
kubeconfig never appears in the transcript"* passed while `docs/secrets.md` uses a
kubeconfig as `secret cp`'s worked example; it knew `at no point` and no other member of
*under no circumstances / at no time / in no case*; it treated **every** quoted span as
somebody else's words, so quoting one word of a live promise deleted the trigger from it;
and
it policed `*.md`, while the retracted sentence was still standing word-for-word in
`cmd_secret_exec`'s docstring and in `commands_persona.py`'s. Both of those are fixed
here, and `charter/**/*.py` docstrings are now in the same scope as the pages.

**Then the same question was asked of that fix, before shipping it, and it fell twice
more.** Put a zero-width space between two letters of the absolute word and the retracted
claim renders exactly as it always did while matching nothing the file knows — the same
trick U+3164 HANGUL FILLER played on a different guard the same night. U+200D, U+00AD and
U+2060 do the job equally well, and an HTML comment does it in plain markdown with no
exotic codepoint at all. So the file undoes what it can of the encoding before it reads:
every character in Unicode category `Cf` is dropped, because "renders as nothing" is what
that category means, HTML entities are decoded, and comments, tags and link wrappers in
all four of markdown's spellings go with them. A **homoglyph** —
one Latin letter swapped for the Cyrillic letter that looks identical — it still cannot
read, and no confusables table ships in the standard library, so that class is refused
rather than matched: a word spelled out of two alphabets fails the build. The exact
spellings live in `tests/test_claims.py`, where a live promise is a fixture rather than a
sentence on a page.

The second fall was self-inflicted, which is the useful part. Narrowing the window between
a verb and its object so that it could not reach across a comma — the fix for a false
positive on the README's containment sentence — made three punctuation marks into bypasses
on the spot, because a pair of commas, a pair of dashes or a parenthesis dropped into the
middle of the retracted claim now split it in two. A reader skips an aside, so the rule
reads each sentence both ways. That is the fifth time in three rounds that a fix for one
spelling opened another, and it is the argument for writing down the next spelling every
time rather than waiting for the next reviewer to find it.

A fourth reviewer then found five more, and the interesting thing about them is that none
needed a new idea. Every one was **a list one entry short**, and four of them had a twin
in the same file that already knew better. The exemption list was matched as a bare
substring anywhere in the sentence — the question round two had taken away from the actor
check three lines above it — so *"…in the transcript, accidentally or otherwise."* carried
a bound phrase while asserting the opposite of a bound, and so did *"…, whatever path you
name."* The window between a verb and its object was a count of four words, which is
exactly what the shipped sentence uses, so one adjective walked past it. The place list
was not pluralised while the credential list was, so the verbatim plural of an existing
fixture passed. The markup list knew one of markdown's four link spellings. And the
encoding rule read the file rather than the page one layer further down: `&#118;` renders
as a `v`, and `&zwnj;` is the entity spelling of a zero-width character, which walks past
the `Cf` drop by not being a `Cf` character in the file at all. A bound the sentence takes
back is no longer a bound, the count is gone in favour of the clause boundary that was
doing the work anyway, and entities are decoded by the standard library's own decoder.

**What the prose test does, said no more strongly than it is true.** It is a regular
expression over prose. It cannot tell whether a sentence is true, and every vocabulary in
it is an open class that will stay incomplete. What it does is refuse the shapes it knows:
a sentence in these files or in charter's own docstrings that makes an absolute claim
about where a value ends up, *in a wording the file recognises*, fails the build unless a
bound *the file recognises* sits in the same sentence **and the sentence does not take it
back**. An earlier draft of this paragraph said it "fails the build if a future sentence
anywhere in these pages makes an absolute claim about where a value ends up without a
bound beside it" — nine such sentences were appended to the README and the suite stayed
green. The draft after that dropped the two qualifications now in bold, and six more
sentences went in. The release note announcing the end of overclaiming has now been
overclaiming about its own guard twice.

Four limits are named in the file rather than left to be found, and the first is the one
the earlier draft of this note got wrong. **The file does not read the rendered page**:
the standard library has no markdown renderer, so what it undoes is an entity decoder,
four markup constructs and two Unicode categories, and round four got past the version of
that list which this note called reading the page "as *rendered*". The answer is not a
fifth construct — it is that a word the page shows which the file does not spell is now
refused outright, whatever assembled it, which is a net under the list rather than another
entry in it. A construct a real renderer joins and this file leaves alone is still
invisible to both. A **homoglyph** it cannot read at all, and that class is refused the
same way, as a word spelled out of two alphabets. It cannot tell a bound *used* from a
bound *mentioned* — a sentence that revokes its limit without quantifying over it,
denying it or coordinating it with the general case reads as bounded, and five such
sentences are fixtures that assert they pass. And a promise that becomes visible only
when a borrowed pronoun antecedent *and* a skipped aside are combined is one the file does
not see — each of those is a guess, and it will not stack two of them.

**Nothing about the vault changed, and nothing needs adopting.** What changed is that the
promise on the front page is bounded where the code bounds it, and that charter's own
source says the same thing its documentation does.
