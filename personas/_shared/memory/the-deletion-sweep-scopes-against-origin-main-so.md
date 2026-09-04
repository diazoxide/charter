# THE DELETION SWEEP SCOPES AGAINST origin/main, SO A STACKED BRANCH IS CH

_2026-09-04 02:27 · persistent_

THE DELETION SWEEP SCOPES AGAINST origin/main, SO A STACKED BRANCH IS CHARGED FOR ITS BASE. PR #873 branched off #862 and its pre-rebase sweep reported 13 survivors; 4 of those belonged to #862, not to #873. After #862 merged and #873 rebased onto main, the same tree reported 0. So on a stacked PR, do NOT read the survivor count as a verdict on your own change until you have rebased onto the merged base — you will otherwise spend effort pinning code someone else already pinned. Two method notes recorded with it: `sorted` over a SET is not a pinnable ordering guarantee (one assertion passes about half the time by luck; use dict.fromkeys in registration order and assert two opposite orders for the same pair), and Python 3.13+ dedents __doc__, so getsource().replace(fn.__doc__, "") silently strips NOTHING — parse with ast instead.
