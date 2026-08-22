# tmux run-shell env inheritance (verified against 3.7c): a run-shell fire

_2026-08-22 18:37 · persistent_

tmux run-shell env inheritance (verified against 3.7c): a run-shell fired with NO -t falls back to the SERVER's raw starting process env (whatever new-session first created it with), NOT to any live 'global environment'. Explicit -t <session-name> (or a real pane id) DOES pick up that session's own set-environment values; -t = (the idiom WheelUpPane's if-shell -F uses) does NOT carry env into a spawned run-shell shell at all -- -t = only resolves for format evaluation, never for a spawned process's environment. So delivering a per-session value to a later run-shell (e.g. CHARTER_SESSION_ID for the frame hotkey menu) needs an explicit set-environment -t <session> call, mirroring _exit_path_env_argv's pattern for CHARTER_FRAME_EXIT.
