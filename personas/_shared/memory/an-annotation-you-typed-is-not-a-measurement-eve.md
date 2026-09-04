# AN ANNOTATION YOU TYPED IS NOT A MEASUREMENT, EVEN WHEN IT SITS ON THE S

_2026-09-03 16:40 · persistent_

AN ANNOTATION YOU TYPED IS NOT A MEASUREMENT, EVEN WHEN IT SITS ON THE SAME LINE AS ONE. From #857: an agent's mutation-check script printed a correct measured count of 9 next to a HARDCODED label reading '(was 9)' — but the true baseline was 10, so a correct result was framed as 'nothing changed' and would have been read as 'the mutation never applied'. The measurement was right; the sentence beside it was a guess typed earlier. Rule: in any verification output, every number a reader will compare against must be MEASURED in the same run — never carried in a string literal from when the script was written. Same family as [[a-number-that-moves-when-the-thing-it-describes-]] (a metric that moves when its subject does not is not measuring it) and [[a-measurement-written-as-a-standing-fact-is-a-va]].
