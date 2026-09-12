# A mutation pin written as an ABSENCE assertion stops pinning silently. M

_2026-09-11 23:15 · persistent_

A mutation pin written as an ABSENCE assertion stops pinning silently. Measured on charter PR 948 (2026-09-11): the pin for doctor.py's 'if unseen else ""' arm asserted that the advice string does NOT contain 'cannot be checked'. Commit f2bcaf9 moved that phrase from outside the join to inside it, so with unseen == [] the collapsed mutant no longer contains the phrase either - the pin kept passing while the guard it named went unpinned, and CI's sweep gate found the survivor again after the round that supposedly closed it. The mutant ships real advice text: charter git-policy --apply with three spaces and a bare dot. Rule: pin an output guard by EQUALITY on the whole sentence, never by the absence of a substring, because any mutation whose output merely stops containing that substring survives.
