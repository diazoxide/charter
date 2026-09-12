# Filing upstream with charter report: the PreToolUse guard refuses the WH

_2026-09-10 11:58 · persistent_

Filing upstream with charter report: the PreToolUse guard refuses the WHOLE Bash line when a live command substitution sits beside `charter report bug`, even though the body comes from --from-file (seen 2026-09-10 chaining RID=<substitution of grep on the draft output> after the draft). Nothing on that line runs. Draft in one call with output redirected to a file, read the `id:` line, then run `charter report send <literal id> --dry-run` in a separate call.
