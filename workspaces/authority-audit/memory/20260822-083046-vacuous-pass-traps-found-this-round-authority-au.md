# Vacuous-pass traps found this round (authority audit): (1) a ONE-LINE ta

_2026-08-22 08:30 · persistent_

Vacuous-pass traps found this round (authority audit): (1) a ONE-LINE target round-trips _drop_index_line's rewrite byte-identically, so assert mtime_ns too; (2) a vault JSON contains no (x.md) link, so an index_drift read test passes whether or not charter read it — put a .md link in the fixture; (3) ONE FIFO helper cannot prove both directions — the blocked writer that proves a write blocks is exactly what satisfies the reader a read-block test needs.
