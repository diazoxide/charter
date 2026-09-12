# Fixing the leak guard's heredoc pre-pass per segment (#973) needs THREE

_2026-09-11 14:41 · persistent_

Fixing the leak guard's heredoc pre-pass per segment (#973) needs THREE clauses, measured against the differential corpus on main a5aa860: strip a body only when the heredoc is QUOTED, its opener (the segment's program via _split_env) is a reader, and NO command in that opener's pipeline is an executor. Dropping the quoted clause regressed nine tests/fixtures/guard_denied_by_main.txt rows (echo hi && cat <<EOF with a live dollar-paren read in the body): main only denied them because the line did not start with a reader, and an unquoted body is expanded by the shell before the reader sees it. Groups, subshells and substitutions on the header line must strip nothing, because { cat <<X; } | bash runs the body.
