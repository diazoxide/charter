# A release now moves five things, not four: the four version files plus '

_2026-08-19 13:27 · persistent_

A release now moves five things, not four: the four version files plus 'charter news stamp <X.Y.Z>', which renames every docs/news/unreleased-*.md onto the version. CI's guard job runs 'charter news --for <version>' and refuses to publish when it exits non-zero, and the announce job turns that same output into the GitHub Release after publish succeeds — so never create the Release by hand, just verify it exists afterwards.
