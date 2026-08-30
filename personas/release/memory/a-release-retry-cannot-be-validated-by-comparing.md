# A release retry cannot be validated by comparing built artifacts against

_2026-08-30 10:31 · persistent_

A release retry cannot be validated by comparing built artifacts against PyPI. hatchling builds charter reproducibly (verified: identical sha256 across fresh checkouts), but docs/news/ ships INSIDE the sdist, so any retry that fixes a news entry rebuilds different bytes by construction — and --ref main picks up whatever else merged. PyPI keeps what it already holds; a retry's business is the Release, not the artifacts.
