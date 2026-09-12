# Pushing under a running review: the test is whether the push INVALIDATES

_2026-09-12 10:31 · persistent_

Pushing under a running review: the test is whether the push INVALIDATES work in flight, not whether the SHA moves. A comments-only commit does not — measurements and mutation results carry over, and the reviewer can verify the claim with 'git diff <old>..<new>'. A behaviour commit does, because then nobody knows which tree a finding belongs to. Corollary measured 2026-09-12: a docstring explaining an unreachable-but-deliberate branch should be pushed DURING a review, not held, because the reviewer is the reader it exists for — holding it shows them exactly the confusion it prevents. And never quote a sweep that describes code nobody will merge: let it run out and re-run on the current head.
