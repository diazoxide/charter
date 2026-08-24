---
version: unreleased
headline: A release leads with the entry that matters, instead of the one named earliest in the alphabet
---

`charter news --for <version>` rendered a version's entries in `sorted(glob("*.md"))`
order, which for a stamped release is alphabetical by slug. `release.yml`'s announce job
pipes that straight into `gh release create --notes-file`, so slug order **was** the
published order — and a slug is a name an author picked while thinking about something
else entirely.

0.52.0 shipped 24 entries. The one that mattered most — a committed MCP server name
interpolated raw into a generated sub-agent's YAML, which could hand any vault on the
machine to an attacker-chosen command — rendered *eighth*, below a 1Password docs
correction. Someone who opened the Release, read two screens and closed it never learned
they needed to upgrade.

The ordering was not a decision. It was whatever the filenames sorted to.

## Two fields, both opt-in

```
---
version: unreleased
headline: …
security: true
lead: true
---
```

The bare word, on the same line as the key, and nothing after it: charter's frontmatter is
flat `key: value` with **no comment syntax**, so a trailing `# …` is part of the value; a
line without a colon is dropped entirely, so the YAML habit of

```
security:
  true
```

never reaches charter at all. A value charter cannot read is reported rather than guessed
at, and so is a field declared with nothing after the colon — writing the key is opting
in, and charter will not quietly decide the author meant "no".

`security: true` says what the entry **is**. It sorts above the ordinary entries of its
version and renders as `security: <headline>` — in the GitHub Release body and in
`charter news` alike, which is the half slug order could never give the offline view. Any
number of a version's entries may declare it.

`lead: true` says where the entry **goes**, and only one entry per version may say it. It
is deliberately not the author's field: 24 entries were staged for 0.52.0 and none of
their authors could see the other 23. "First" is a claim that needs the whole release in
view, so it belongs to the release engineer at `charter news stamp` time.

Declare neither and nothing changes — entries sort by filename exactly as they always
have. That is why 24 shipped entries needed no edit.

## Both views, one answer

The sort lives in `news.all()`, which the Release body and `charter news` both read
through. That is the point rather than an implementation detail: the shipped entry is
deliberately the single source for both, and a fix that taught only the Release body to
lead with a security note would have forked them just as surely as hand-editing the
Release would have.

## A contradiction stops the tag, not the reader

Two entries in one version both claiming `lead: true` is not a tie to be broken quietly —
that is the same "decided by an accident" this exists to end. `charter news --for` refuses
such a version, and since that call *is* the release workflow's pre-publish guard, the
release stops before there is a Release body to be wrong.

An ordering field charter cannot read stops it too. The fields take `true` or `false`,
and anything else — `yes`, `1`, a full-width `ｔｒｕｅ` — is reported by name rather than
read as false.
The obvious implementation is a set of words meaning yes, and it fails the way this
codebase has been failed six times before: an author writes the seventh spelling, charter
reads it as "no", and the entry sinks to the bottom of the notes in silence — the exact
defect the field was added to prevent. So the question asked is not *which word is this*
but *was this value understood*.

**Declaring the field and leaving the value off is that same failure, and it very nearly
shipped inside the fix.** `security:` with the value indented onto the next line — the
YAML habit — leaves charter holding the key with an empty value, and the first cut read
"empty" and "never written" as one thing, because the code asked for the value and treated
a blank one as a missing key. So an author who *did* declare a security fix got the
bottom of the notes, exit 0, and not one word about it: #486 restored through the field
added to prevent it. Declaring a field is now a fact charter reads separately from the
value in it, and an empty one is refused with a sentence naming the line the value went
onto.

`charter news`'s range view warns instead of refusing. A reader catching up should not
lose nineteen entries because a twentieth has a typo in its frontmatter.

## Three limits, filed rather than folded in

Write the field's **name** in any other case — `Security: true` — and none of the above
happens. charter's frontmatter is looked up by an exact key, so the entry declares nothing
as far as the sort is concerned, `charter news --for` is satisfied, and the entry sinks
exactly as #486 described. That is the value half's failure reached through the key half,
and fixing it means changing how `persona.parse`'s dict is read by every caller of it, so
it is **#503**. Write the field **twice** in one entry and charter keeps the last line
without saying so, so `security: true` followed by a stray `security: false` reads as
false — the same collapse one row over, and **#509**.

And the messages above name the entry by filename, which is committed text, contained for
its value but not for its name — a `.md` whose name holds a newline forges a line of
charter's own report. Two of the four sites predate this change, in `news stamp`, so all
four move together in **#502**.

## What to do about it

Nothing. If you write news entries, `security: true` is now available and worth using
when it is true. 0.52.0's entry for the MCP server-name fix has been marked `lead: true`,
so `charter news --for 0.52.0` now shows it first — the published Release body for that
version was rendered before any of this existed and is unchanged.
