# A heredoc pre-pass must end a body exactly where bash does: a line EQUAL

_2026-09-11 16:11 · persistent_

A heredoc pre-pass must end a body exactly where bash does: a line EQUAL to the delimiter (a <<- form ignoring only leading tabs), and in an UNQUOTED body a trailing odd backslash splices the next line so that line cannot terminate. The lenient line.strip() == delim match looked safe (ends earlier, shows more) but measured unsafe on bash, zsh and dash: in bash <<'A' && cat <<'B' a body line ' A' ended the KEPT shell body early, the real read that spilled past was taken as B's reader body and stripped, and a secret ran while main denied it (#973 round 1, 2026-09-11). The differential corpus had no multi-heredoc row, so only a measurement caught it.
