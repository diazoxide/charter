# A MUTANT CAN LEAVE EVERY LOOP IN ITS OWN FUNCTION TERMINATING AND STILL

_2026-08-31 09:02 · persistent_

A MUTANT CAN LEAVE EVERY LOOP IN ITS OWN FUNCTION TERMINATING AND STILL RUN AWAY, BECAUSE WHAT IT CORRUPTS IS A CURSOR RETURNED ACROSS A FUNCTION BOUNDARY. Measured 2026-08-31 on charter/hooks.py:3738 [drop-if]. The author predicted this mutant merely spins, reasoning correctly that its loop is inside _heredoc_header where a rescan from j=0 always meets the '<' of '<<' and breaks. The reasoning was right and the conclusion wrong: with the guard deleted _heredoc_header RETURNS a j pointing BACKWARDS, before the '<<' it was called on; _live_substitution assigns i = j, the cursor moves backwards, re-finds the same '<<', and calls the header forever, appending to its pending-heredoc list each pass. Measured on the real mutant against a malformed input already in the suite: 294 MB in 4.1 s, about 4,274 MB/min — the FASTEST of that branch's four runaways, not the harmless one. TWO RULES. (1) A static 'does this mutant contain a runaway' check fails in the FAIL-OPEN direction on any scanner, because a scanner is mostly made of cursors returned across boundaries. What found it was applying all 118 mutations under a wall clock — a few minutes on a laptop. (2) VERIFY THE REAL MUTANT, NOT A RECONSTRUCTION. The author's hand-written reconstruction was faithful to the FUNCTION and that is exactly why it missed: the defect lived in the contract with the caller. Related: [[the-sweep-cannot-name-the-mutation-that-killed-a]].


## Recorded the second time, because writing this memory reproduced the defect

The first attempt to save this note ran `charter persona remember ... "...appending to <backtick>pending<backtick> each pass..."` — backticks inside a DOUBLE-QUOTED shell argument. zsh ran `pending` as a command, printed `command not found`, and substituted its empty output, so the saved text read "appending to  each pass" with the word gone.

That is #703 exactly: the defect the whole #710 guard exists to prevent, committed while documenting #710's own findings, by the person coordinating it. It cost a word here; the same slip in `gh issue create --body` published the operator's environment to a public repository earlier in the same session.

**The remedy is the one the guard names: `--body-file -` with a QUOTED heredoc, or a quoted-heredoc `python3 -` for file edits.** Never put a backtick inside a double-quoted shell argument, regardless of where the text is going. And note what makes it dangerous: the failure was SILENT in the saved artifact — only the stray `command not found` on stderr revealed it, and nothing would have if the word had happened to name a real command.
