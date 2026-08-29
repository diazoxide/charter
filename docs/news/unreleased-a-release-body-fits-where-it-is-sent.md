---
version: unreleased
headline: A release stops being able to publish to PyPI and then lose its notes — the body is bounded to what GitHub accepts, and no note is dropped to make it fit
---

`release.yml`'s announce job pipes `charter news --for <version>` straight into
`gh release create --notes-file`, and GitHub's create-release API refuses a body over
**125,000 characters** with `body is too long`. It does not trim to fit and `gh` does not
either — it forwards the refusal. So a version whose entries render past that limit did not
publish shorter notes. It published **none**.

**The job that fails is behind the one that cannot be undone.** The order is guard, test,
build, *upload to PyPI*, then create the Release. By the time GitHub refuses, the upload has
happened, and the documented retry cannot repair it: `gh workflow run release.yml -f
version=<X.Y.Z>` re-enters `publish`, `pypa/gh-action-pypi-publish` is called without
`skip-existing`, and PyPI rejects a version it already has — so `announce` is never reached
on any subsequent run. The only exit left is the one the release charter forbids: writing
the Release body by hand, which forks the published notes from the shipped entry that is
supposed to be their single source.

The 69 entries staged for this release rendered to 333,917 characters. 0.52.0 published at
111,723 — 3,277 short of the refusal — and nothing in the repository remarked on it, because
nothing was looking. Entries accumulate one pull request at a time, each author sees their
own and no other, and the total crosses on an ordinary afternoon with no pull request to
blame.

**The guard was not wrong, and rendering was never the failing step.** `guard` already
refuses a version whose notes cannot be *rendered*, and it asks `charter news --for` — the
same call `announce` makes — precisely so that "the guard passed" and "the notes render"
cannot disagree. That call exits 0 on a 300,000-character body, because producing the string
is exactly what it was asked to do. The claim nobody was making is that the string **fits
where it is about to be sent**.

## What changed

`news.render_body` is bounded. Whenever the notes fit, they render exactly as they always
have, byte for byte — which is every release but two. Past that, the notes that fit render
whole and the rest become their headline and a link to the note itself, with a heading
between them that says how many, how long, and what the limit is.

```
## 34 of these 69 notes are listed by headline only

Rendered whole, 69 notes come to 333,917 characters, and GitHub refuses a release body
over 125,000.

**Every note this version shipped is in this list.** 35 are above in full; the 34 below
are a headline and a link. …
```

**Nothing is dropped and nothing is truncated**, and that is the half worth checking rather
than trusting. Cutting the string at 125,000 characters would satisfy the limit and is
indistinguishable, to a reader, from a release that shipped a dozen fewer things. So every
entry keeps its own heading in its own place in the order, every elided entry carries a link
to the note holding its text, and the tests assert those properties on the entries this
repository is actually carrying — not on a fixture, which would only prove that
`render_body` can count.

**The cut is one point in the order, not a per-entry decision.** A greedy fill that skipped
the big notes and kept packing the small ones would give an ordinary note its body while a
security note above it lost one — #486 wearing a size limit. There is no second rule inside
the bound that protects security entries: there is one order, `news.all()`'s, and the notes
that lead are the notes that keep their bodies.

## And the assertion moved to the cheap end

A body that cannot be brought under the limit even with every note reduced to a headline is
refused rather than cut, by `charter news --for` — which is the command `guard` runs. The
failure therefore arrives before `test`, `build` and `publish`, instead of after the upload.
`release.yml`'s annotation names that third cause alongside the two it already named.

Nothing to adopt: this is charter's own release path, and it is fixed for every plane the
moment this version publishes.
