# tools/sweep.py reports 'not text.strip()' swapped for 'lstrip' as a surv

_2026-09-11 07:16 · persistent_

tools/sweep.py reports 'not text.strip()' swapped for 'lstrip' as a survivor, and it is an EQUIVALENT mutant: both are empty exactly when the text is all whitespace, so no test can pin it. Remove it by construction: split once ('words = text.split()'; 'if not words' for empty, 'len(words) == 1' for one word). The sweep deliberately leaves the no-argument split/rsplit swap alone (SYNONYMS note, tools/sweep.py). Measured 2026-09-11 on chat-handoff Task 1, where the same change stopped a single-word refusal naming a trailing newline.
