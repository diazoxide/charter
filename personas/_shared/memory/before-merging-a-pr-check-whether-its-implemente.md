# Before merging a PR, check whether its implementer still has BACKGROUND

_2026-09-11 21:45 · persistent_

Before merging a PR, check whether its implementer still has BACKGROUND work in flight that will produce more commits — a local deletion sweep is the usual one. On 2026-09-11 PR 974 was merged on a green CI and a READY review while its implementer's sweep of the head was still running; the sweep then named 7 more unpinned guards in that PR's own new code, and the implementer pushed pins and deletions to the branch AFTER the squash merge, so they were not on main and needed a follow-up PR. Either wait for the sweep before merging, or merge deliberately and tell the implementer at once that anything further goes in a follow-up PR from a fresh piece off main, re-verified on the new base (a squash merge changes the context its commits applied to). Ask the implementer, in the message that asks for its merge-ready report, to state explicitly what it still has running.
