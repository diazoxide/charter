# tmux argv, pinned against 3.7c by rejection: passing a command to new-se

_2026-08-22 00:11 · persistent_

tmux argv, pinned against 3.7c by rejection: passing a command to new-session as SEPARATE arguments is NOT shell-interpreted (printf 'hello;touch INJ' created no file), while the same text as ONE joined string IS (the file appeared). So 'never join argv' is a security rule, not a style preference — the same input class as the gh -F fix, since workspace/repo/branch/persona names all come from committed files or .git/HEAD. Also pinned: an attached tmux new-session returns 0 whatever its command exited with, so an exit code must be carried out of band via remain-on-exit plus a PANE-SCOPED pane-died hook (unscoped it fires for any pane and reports a dead panel as the agent's exit).
