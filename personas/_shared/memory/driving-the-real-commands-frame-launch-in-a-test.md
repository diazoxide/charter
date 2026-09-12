# Driving the REAL commands_frame._launch in a test (e.g. through open_in_

_2026-09-11 05:24 · persistent_

Driving the REAL commands_frame._launch in a test (e.g. through open_in_background on a _tmuxreap socket) needs commands_frame._spawn_gather stood in: the launch forks a detached 'charter frame-gather' with start_new_session=True and tests/_planeguard.py refuses it (BackgroundCharterChild). Measured 2026-09-11 on chat-handoff Task 1. Also measured that day, each harness's first-message argv from its own --help: Claude Code 2.1.268 'claude [options] [command] [prompt]' (interactive by default; -p/--print is non-interactive); codex-cli 0.147.0 'codex [OPTIONS] [PROMPT]'; opencode 1.18.23 positional is [project], prompt is --prompt.
