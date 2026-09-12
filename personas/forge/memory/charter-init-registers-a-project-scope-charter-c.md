# charter init registers a project-scope charter@charter entry in the CALL

_2026-09-11 03:24 · persistent_

charter init registers a project-scope charter@charter entry in the CALLER's real ~/.claude/plugins/installed_plugins.json whenever claude is on PATH -- this happens even for a throwaway plane built purely to reproduce a bug, and it is the operator's real Claude Code config, not sandboxed. Before charter init'ing any throwaway/scratch plane: strip every PATH dir with an executable claude, and set XDG_CONFIG_HOME to a throwaway dir. main ships docs/assets/isolated-init.sh (merged via #958) that does exactly this -- read it with git show origin/main:docs/assets/isolated-init.sh and source/copy it rather than calling charter init raw.
