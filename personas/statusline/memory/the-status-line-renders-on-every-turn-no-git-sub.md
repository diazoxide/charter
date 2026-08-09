# The status line renders on EVERY turn: no git subprocess and no network

_2026-08-09 22:54 · persistent_

The status line renders on EVERY turn: no git subprocess and no network on the render path. Branches come from .git via util.branch_of; forge state from a cache another process fills.
