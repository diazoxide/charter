# release.yml's tag/version check is gated 'if: startsWith(github.ref, ref

_2026-08-26 19:42 · persistent_

release.yml's tag/version check is gated 'if: startsWith(github.ref, refs/tags/v)', so on the workflow_dispatch retry path it is SKIPPED, not passed — and a skipped step reports success in GitHub Actions. The pypi environment has NO protection rules (gh api repos/diazoxide/charter/environments -> rules=NONE), so the environment name is load-bearing for the OIDC claim but gates nothing. Combined: between a version bump merging to main and the tag being pushed, a workflow_dispatch publishes an untagged, uncross-checked version with no human checkpoint. Filed as #558. Consequence for dependabot: the four action bumps (#478-481) cannot be rehearsed, because a dispatch run does not stop at a gate — it publishes.
