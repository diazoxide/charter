# A hint is only correct if the command it NAMES clears the row it appears

_2026-09-12 10:31 · persistent_

A hint is only correct if the command it NAMES clears the row it appears on — and the only evidence is running it. Measure end to end: build the state, confirm the row warns with that hint, run the named command through its own entry point, re-read the row. Reading the predicate proves nothing. Measured 2026-09-12 on charter doctor's handoff-gate row: both 'charter workspace reinit <ws>' states (a workspace directory, and a guest checkout inside one) and the 'charter guard ask' state all cleared. Enumerate the states from the code that decides them (here _reinit_target answers non-None only for a workspace dir or a guest tree root) so the table is complete rather than merely unfalsified.
