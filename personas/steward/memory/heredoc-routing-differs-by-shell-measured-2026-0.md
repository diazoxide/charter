# Heredoc routing differs by shell, measured 2026-09-11 (bash 3.2, zsh 5.9

_2026-09-11 13:56 · persistent_

Heredoc routing differs by shell, measured 2026-09-11 (bash 3.2, zsh 5.9, dash): bodies always follow the << operators in order across the WHOLE line, but zsh (the Claude Code harness shell) has MULTIOS on, so a segment with two heredocs gets BOTH bodies, and 'x | bash <<EOF' feeds bash the pipe AND the heredoc; bash/dash keep only the last redirection. bash 3.2 also ends a heredoc at 'EOF)' inside $( ) where zsh/dash do not, and '<<$'\''EOF'\''' ends at EOF in bash/zsh but at $EOF in dash. A charter guard modelling heredocs must treat every heredoc on a segment as received, and must not strip a body whose header has a substitution or a $ in its delimiter.
