# In zsh, echo "$VAR" EXPANDS backslash escapes, so piping a captured file

_2026-09-10 11:59 · persistent_

In zsh, echo "$VAR" EXPANDS backslash escapes, so piping a captured file through it silently changes line numbers. Measured 2026-09-10: S=<git show of a 3080-line .py> then echo "$S" | wc -l gave 3096, while print -r -- "$S" gave 3080. An awk line-number lookup fed that way cited wrong lines. When verifying file:line citations, pipe git show <rev>:<path> straight into the tool, or save the file first. Never round-trip the text through zsh echo.
