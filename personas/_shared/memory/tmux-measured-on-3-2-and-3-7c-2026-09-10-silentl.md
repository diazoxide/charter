# tmux (measured on 3.2 and 3.7c, 2026-09-10) silently drops a TRAILING se

_2026-09-10 23:29 · persistent_

tmux (measured on 3.2 and 3.7c, 2026-09-10) silently drops a TRAILING semicolon from an argument passed to new-session or new-window: it reads it as a command separator, exits 0 and prints nothing. So 'run the tests;' reaches the child as 'run the tests'. A semicolon mid-text, quotes, backticks, dollar-paren, tabs and newlines all survive byte-for-byte. The exact fix is to send the text with the final semicolon replaced by backslash-semicolon. tmux also refuses any command message over 16364 bytes with 'failed to send command' (not 'too long'). With a 122-byte plane root that leaves about 15920 bytes of argument text, and every extra byte of root or cwd costs one byte.
