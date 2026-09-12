# Claude Code 2.1.268's ask rule Bash(charter handoff *) does NOT match a

_2026-09-11 13:51 · persistent_

Claude Code 2.1.268's ask rule Bash(charter handoff *) does NOT match a quoted or split SECOND word: charter 'handoff', charter "handoff" and charter h""andoff all ran a handoff with no prompt (measured 2026-09-11, mock Messages API, --bare -p --permission-prompts none, manual and bypassPermissions identical, no-rule control ran them too). It DID match (prompted for) a quoted or escaped FIRST word (\charter, 'charter', "charter", char""ter, ch\arter), two spaces, a tab and a backslash-newline between the words. charter + U+00A0 + handoff got no prompt but the shell (zsh) found no such command. So a host rule is not a reliable spelling boundary; charter's A7 compares the source spelling instead (both tokens bare, one ASCII space apart, _Tok.start offsets).
