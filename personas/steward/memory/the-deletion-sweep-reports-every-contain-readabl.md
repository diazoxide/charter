# The deletion sweep reports every contain.readable call on a value that c

_2026-09-12 00:46 · persistent_

The deletion sweep reports every contain.readable call on a value that cannot carry a control byte today (a profile name already past NAME_RE, a built-in word) as a survivor: on PR #978 (ce7f5d8, 2026-09-12) several of the ten survivors from the first three shards were exactly that. Precedent (survivor [12], ruling 35): keep the escape and pin it by handing that layer a hand-built unvalidated value, e.g. a ProfileSet whose name holds chr(13), asserting the escaped spelling appears and no raw CR does. One table-driven test that puts a CR into the one value each refusal sentence repeats pins every sentence at once, so later shards find nothing new there. Build raw control characters with chr(13) in test code, never a typed unicode escape.
