# When a release branch is overtaken by a PR that EDITS the same docs/news

_2026-09-04 12:29 · persistent_

When a release branch is overtaken by a PR that EDITS the same docs/news entries the branch has stamped, do NOT rebase — the rename+edit conflict resolves into a stamped copy of the stale prose, which publishes silently. Reset --hard to origin/main and REPLAY the release (bump the four files, re-run news.stamp, re-apply lead:), then force-push with --force-with-lease. Replaying also picks up entries staged after the branch was cut, which a rebase would leave behind. Used on 0.56.0 after #883 corrected eight entries the branch had already stamped.
