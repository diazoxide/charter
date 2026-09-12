# Heredoc delimiter reading, measured 2026-09-11 with /bin/bash 3.2.57 and

_2026-09-11 16:39 · persistent_

Heredoc delimiter reading, measured 2026-09-11 with /bin/bash 3.2.57 and /bin/zsh on macOS: <<EO'F', <<'EO'F, <<"EO"F and <<\\EOF all terminate at a line reading EOF, and all four bodies are LITERAL ($HOME is not expanded) — quoting or an escape ANYWHERE in the delimiter word both joins the word and stops expansion. A regex such as charter's hooks._HEREDOC_RE yields EO for the first three and matches nothing for <<\\EOF, so it can never find those terminators. The consequence for guards: a wrong delimiter errs SAFE when the body is being hidden from a guard (the terminator is never found, the body stays visible) and errs DANGEROUSLY when the body is being DROPPED (it drops to end of input and swallows the real commands after the heredoc). hooks._heredoc_header reads all four correctly; anything that drops a body must take the delimiter from it, never from the regex.
