# release.yml's publish passes skip-existing ONLY on workflow_dispatch (#6

_2026-08-30 10:31 · persistent_

release.yml's publish passes skip-existing ONLY on workflow_dispatch (#673). Unconditional is a trap: a tag deleted and re-pushed over a changed tree with the version unbumped would then skip every file, find the Release already standing, and report GREEN having shipped nothing while pip still serves the old code. On the tag path a version PyPI already holds means bump to a patch, not retry.
