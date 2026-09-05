---
version: unreleased
headline: A news headline wrapped in quotes is refused at the release gate, instead of publishing its quotes inside the heading
---

`docs/news/` frontmatter opens with `---` and closes with `---`, which is exactly what YAML
frontmatter looks like. It is not YAML. `persona.parse` reads flat `key: value` and takes
everything after the first colon verbatim, so an entry written like this:

```
headline: '`charter workspace reinit --all` counts repairs and workspaces apart'
```

published like this:

```
### '`charter workspace reinit --all` counts repairs and workspaces apart'
```

## The author was not being careless

A headline in this repo usually starts with a backtick, and a backtick is a reserved
indicator in real YAML. Anyone who believes this frontmatter is YAML — and everything about
how it looks says so — is *correct* to quote that headline. The format punished knowing
YAML, and it punished quietly: the file is well-formed, the entry sorts correctly,
`charter news --for` exited 0, and the only person who ever saw the quotes was a reader of
the published notes.

**0.56.0 shipped six of them.** Nobody noticed until 0.57.0 was being cut, when one entry
was caught by hand by somebody who happened to look. Neither the suite nor the release
guard had an opinion.

## Refused, not silently unquoted

The other candidate was to strip a matched pair of quotes when reading the value. It is
cheaper and it would have fixed the six retroactively — and it is the wrong half of the
trade. Stripping honours exactly one YAML habit while charter goes on dropping continuation
lines, ignoring anchors and keeping backslashes: half a spec, with nothing to tell the next
author which half they have. `news._flag` already refused this shape once, for a different
YAML habit — a value indented onto the line below — and answered it with a sentence naming
the habit rather than by learning to read it. And a headline that legitimately ends in a
quote would become unwritable *silently*, where a refusal at least says so.

So charter says what the format is:

```
$ charter news --for 0.58.0
✗ the news for 0.58.0 quotes a value charter does not unquote:
  0.58.0-a-thing.md: `headline:` opens and closes with ' and charter kept both. This
  frontmatter is not YAML — it is flat `key: value`, and everything after the first colon
  is the value — so '…' publishes as '…', quotes and all…
```

## Where it is asked

Three surfaces, one function — `news.quoted_values`, so they cannot disagree:

- **the suite**, over every entry committed to the tree, which is the one an author meets
  and the only one that runs before merge;
- **`charter news --for <version>`**, which is `release.yml`'s pre-publish guard and the
  call whose stdout becomes the Release body — the last place 0.56.0 could have been
  stopped;
- **not the range view.** `charter news` warns a reader catching up about entries charter
  cannot honour, and this is not one of those: charter reads a quoted headline perfectly
  and ships exactly the bytes the file holds. Warning there would scold every user whose
  upgrade spans 0.56.0, on every run, about six files nobody is going to change.

## The six 0.56.0 entries are reported, not rewritten

They are what the published 0.56.0 Release says. Correcting them in the repo would fork
this tree's copy of a stamped release's notes from the notes themselves, leaving two
documents that claim to be the same one. `news.quoted_values` names all six to anyone who
asks — `charter news --for 0.56.0` refuses today, which is the proof the gate would have
caught them — and the suite pins exactly those six by name, so a seventh goes red and a
correction to one of the six goes red too, as a decision about the published Release rather
than a tidy-up.

## Nothing to adopt

The check runs in this repo's own CI and release guard. A plane that consumes charter has
nothing to take up.
