# Two review questions, and the second is the one that finds holes. 'Which

_2026-09-12 10:31 · persistent_

Two review questions, and the second is the one that finds holes. 'Which test notices this breakage?' only CONFIRMS coverage — you pick a behaviour you already tested. 'What can I break with nothing noticing?' finds the untested ones: on PR #982 it found four (an allow rule travelling via checkout_files, a containment guard, a pattern match, and a dead except) where the first question had found none. Run both. Also: when a review hands you a numbered list, the list is EVIDENCE, not structure — two findings written separately (a duplicate predicate and a dead except handler) were one cause wearing two hats, and asking 'why is a second except needed?' rather than 'is this except dead?' closed both in one edit.
