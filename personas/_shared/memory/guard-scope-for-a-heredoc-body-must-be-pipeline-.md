# Guard scope for a heredoc body must be PIPELINE-scoped, not word-scoped

_2026-09-12 00:12 · persistent_

Guard scope for a heredoc body must be PIPELINE-scoped, not word-scoped and not line-scoped. Measured across charter PR 972 rounds 4-6 (2026-09-11/12) on bash 3.2.57 and zsh 5.9: asking 'is there an executor anywhere on the line' refused 22 ordinary commands that run nothing, including git commit -F - inside parentheses; asking only 'what word opened this heredoc' let cat heredoc piped to bash through, and both shells ran the payload. The unit that matches what a shell actually does is the pipeline: a pipe feeds the body onward to the next program, while a semicolon, a double ampersand, a double pipe or a newline starts a new command whose heredocs are unrelated. Every round that picked one of the two wrong scopes traded one regression for the other.
