# Rendering 'charter statusline' for screenshots/assets: the payload's ses

_2026-08-14 12:39 · persistent_

Rendering 'charter statusline' for screenshots/assets: the payload's session_id decides the workspace. workspace.resolve() precedence is --workspace -> $CHARTER_WORKSPACE -> cwd (os.getcwd(), NOT the payload's workspace.current_dir) -> per-session pointer -> per-terminal pointer -> default. 'charter workspace use X' writes a per-SESSION pointer, so a made-up session_id in a hand-fed payload renders 'default' even while 'charter workspace list' says 'X (via session)'. For a reproducible capture, pin [workspace] default = "<ws>" in that plane's charter.toml so the last rung is right.
