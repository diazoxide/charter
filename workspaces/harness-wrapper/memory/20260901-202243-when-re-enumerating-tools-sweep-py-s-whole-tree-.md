# When re-enumerating tools/sweep.py's whole-tree plan to check a rule cha

_2026-09-01 20:22 · persistent_

When re-enumerating tools/sweep.py's whole-tree plan to check a rule change, keep the SWEEP root and the SOURCE root separate: tools/sweep.py is itself one of the swept sources, so running the changed rule over its own changed file confounds 'mutations the rule stopped planning' with 'mutations the fix's new lines added'. On #797 the rule removed 2 (11,829 -> 11,827 over main's sources) while the branch's own tree measured 11,831 — two MORE than main. Both numbers are true and only the first one is about the rule.
