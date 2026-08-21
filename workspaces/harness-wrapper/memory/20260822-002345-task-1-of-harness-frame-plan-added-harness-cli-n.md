# Task 1 of harness-frame plan: added Harness.cli_name, Harness.binary, an

_2026-08-22 00:23 · persistent_

Task 1 of harness-frame plan: added Harness.cli_name, Harness.binary, and Harness.launch_argv() to charter/harness/base.py, and set cli_name/binary on ClaudeCodeHarness (claude), CodexHarness (codex), OpenCodeHarness (opencode). launch_argv() returns a list (never a joined string) so tmux never shell-interprets operator arguments. Added tests/test_harness_launch.py (4 tests; dropped the brief's defective shadow-collision test per instruction — its assertion removed the value before checking membership so it could never fail). Commit 3536511 on branch harness-frame. Full suite: 3047 tests, OK.
