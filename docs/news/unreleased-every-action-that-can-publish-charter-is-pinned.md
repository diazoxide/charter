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

**All thirteen `uses:` in both workflows now name a commit**, with the tag it resolved to
beside it:

```yaml
- uses: pypa/gh-action-pypi-publish@dc37677b2e1c63e2034f94d8a5b11f265b73ba33 # v1.14.2
```

**The rule that gets tested is not "the string does not say v4".** `tests/test_workflows.py`
asks the property: given only this ref, can the bytes it resolves to change without a
commit landing in this repository? A full commit SHA is a content address, an image digest
is a content address, a path inside this repo moves only when this repo does — everything
else is a promise by a third party, however it is spelled. The scanner walks every YAML
file under `.github/` rather than the two workflows that exist today, and it fails closed:
it counts `uses:` at the byte level and refuses to pass unless it produced a ref for every
one, so a spelling it does not understand fails the suite instead of being skipped by it.

**A pin freezes a security fix out as effectively as it freezes an attacker out**, so
`.github/dependabot.yml` now opens the pull request that moves each one. It opens it; a
person still reads the diff and merges it. Same bargain `charter update` makes with the
operator, for the same reason.

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
