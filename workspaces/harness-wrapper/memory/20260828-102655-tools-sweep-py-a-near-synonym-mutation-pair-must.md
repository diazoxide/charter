# tools/sweep.py: a near-synonym mutation pair must be type-correct on EVE

_2026-08-28 10:26 · persistent_

tools/sweep.py: a near-synonym mutation pair must be type-correct on EVERY receiver, or it manufactures false pins. index/rindex is not (list has index, only str has rindex) and a module receiver is not (shlex.split has no rsplit) — both were caught by checking the table against the tree rather than trusting it. Also: retune-string is scoped to read positions because mutating every string took charter/ from 7006 to 14801 mutations, the difference all prose.
