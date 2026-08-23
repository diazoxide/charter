# Nudge standard (#371, PR 379): a prompt is worth its interruption only i

_2026-08-22 22:50 · persistent_

Nudge standard (#371, PR 379): a prompt is worth its interruption only if it changes what happens, and for a hook 'ask' that evidence is a DECLINE — which produces no PostToolUse and is indistinguishable from an interrupted turn or an ended session. So an ask can never be justified by outcome data; only DISAPPROVAL of the alternative can justify it. Clone-commit nudge deleted on that basis (471 asks, 97/98 approved, trigger condition WAS the prescribed workflow). GOTCHA found while deleting: _ask_mark_take was wired to posttooluse_bash ALONE, so the two surviving nudges (Task|Agent, Write|Edit|MultiEdit) could never record an approval — deleting the only Bash ask would have pinned 'approved M' at 0 forever. Take half now in one _ask_approved helper called from all three PostToolUse handlers.
