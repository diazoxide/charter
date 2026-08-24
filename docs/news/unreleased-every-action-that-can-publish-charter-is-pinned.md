---
version: unreleased
headline: Every action in charter's CI is pinned to a commit, starting with the two that stand next to the publishing token
---

charter publishes to PyPI with Trusted Publishing, so there is no API token anywhere: not
in the repo, not in Actions secrets, not on a laptop. That is worth having and it is not
the whole question. What it means is that nothing needs to *steal* a credential to publish
charter — the `publish` job mints a genuine OIDC token on demand, and PyPI verifies a claim
about repo + workflow + environment that is entirely true. **So whatever code runs in that
job can publish charter.** The credential question becomes a code question.

Until now that job named its code by floating refs:

```yaml
- uses: actions/download-artifact@v4                # a tag its owner can retarget
- uses: pypa/gh-action-pypi-publish@release/v1      # a BRANCH head, not a tag at all
```

`release/v1` has no `refs/tags` entry — it is `refs/heads/release/v1`, one force-push away
from being different code, inside a job where `ACTIONS_ID_TOKEN_REQUEST_URL` is ambient.
That is the mechanism of CVE-2025-30066 exactly, and SHA pinning is what separated the
victims from everybody else.

**All fourteen `uses:` in both workflows now name a commit**, with the tag it resolved to
beside it:

```yaml
- uses: pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33 # v1.14.2
```

**The rule that gets tested is not "the string does not say v4".** `tests/test_workflows.py`
asks the property: given only this ref, can the bytes it resolves to change without a
commit landing in this repository? A full commit SHA is a content address, an image digest
is a content address, a file **tracked in this repo** moves only when this repo does —
everything else is a promise by a third party, however it is spelled.

**And the check reads YAML rather than grepping for `uses:`, because the first version of
it grepped and that was a hole.** It matched the key with a regex and cross-checked itself
by counting the literal bytes `uses:`. Both are one spelling. `- "uses": evil/action@main`
contains neither, parses to a genuine step of the `publish` job, and passed the whole file
green — as did `- 'uses':`, the explicit key `- ? uses` / `: …`, and `"\x75ses"`. So the
test now carries a small YAML reader for the subset these files are written in, and
anything outside that subset — a flow mapping, an anchor, an alias, a merge key, a tag, an
explicit key, a `uses:` whose value is on the next line — raises by name instead of being
skipped. `TheKeyHasManySpellings` is the corpus of every spelling that used to get through,
and it asserts *which* outcome each one gets, so no case can pass because some other check
happened to trip. The second hole was the same mistake in a second place: the file list came
from a tree walk with a hardcoded skip list (`node_modules`, `dist`, `.venv`), so
`uses: ./node_modules/probe` was waved through as "in this tree" while the skip list
guaranteed its `action.yml` was never read. The file list now comes from `git ls-files`, a
local `uses: ./x` has to resolve to a file in it, and a local action that is not tracked
here is not a pin — nor is one reached through a symlink, which git stores as a name and
not as bytes.

**A pin freezes a security fix out as effectively as it freezes an attacker out**, so
`.github/dependabot.yml` now opens the pull request that moves each one. It opens it; a
person still reads the diff and merges it. Same bargain `charter update` makes with the
operator, for the same reason.

The same rule now covers `container:` and `services:` images, which were never in these
files and were never checked either: `image: node:18` runs a third party's bytes inside the
job exactly as a step does, and a tag is a tag.

Two things this deliberately does not claim. It cannot check that a pinned SHA is a real
commit of the repo beside it, or that its `# v1.14.2` comment is honest — both need the
network, and no test in this suite makes a network call; a wrong-but-well-formed SHA fails
in CI on the next run, loudly. And pinning says nothing about a `run:` step that pipes a
script off the internet. There is none in these files, and pinning would not save you from
one if there were.

`release.yml`'s header used to end *"so a leaked secret cannot be used to publish charter"*
— true, and answering the smaller question. It now says which question is the real one and
what this file does about it.

Nothing to adopt: this is charter's own pipeline, and it applies from the next release
cut.
