# A guard that reads prose must ship its attack corpus as test data AND an

_2026-08-30 14:42 · persistent_

A guard that reads prose must ship its attack corpus as test data AND an executed list of what it cannot catch. Two measured traps from charter #689: (1) a 'not a version number' regex written as (?![.\d]) silently skips every integer that ENDS a sentence — use (?!\.\d) plus (?!\d) instead; (2) scoping a doc check to 'the sentence containing the literal `key`' matches markup, not the property — scope by the WORD (pad/pads/padded/padding, lookbehind to exclude trackpad) over the paragraph plus one sentence either side, flattened across paragraph breaks. If your diff is tests+docs only the sweep gate plans 0 mutations, so hand-mutate and verify sha256 on apply and restore.
