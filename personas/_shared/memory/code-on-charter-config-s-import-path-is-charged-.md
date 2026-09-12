# Code on charter.config's import path is charged by the deletion sweep to

_2026-09-11 23:17 · persistent_

Code on charter.config's import path is charged by the deletion sweep to the WHOLE suite, so CI's sweep cannot finish. Measured on PR 978 (harness profiles task 1, 2026-09-11): config.derive called profiles.derive at import, so 117 of 156 sweep mutations sat in profiles.py and each selected ~347 test modules. Every CI sweep shard hit its 60-minute timeout-minutes and the verdict could only be 'no verdict'; the local sweep died of memory pressure at 27/156. Fix (ruling 43): keep feature code off the import path and resolve it lazily in the surfaces that use it (memoised per process). This also keeps the work off every hook process. It extends the #923/#926 lesson: when a line cannot be measured, move the code, not the bar.
