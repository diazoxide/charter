# tmux 'server exited unexpectedly' rc 1 in a test fixture is always #713'

_2026-09-03 08:43 · persistent_

tmux 'server exited unexpectedly' rc 1 in a test fixture is always #713's mechanism: kill-server leaves the socket FILE, so the next new-session is a client that CONNECTS to a retiring server instead of building one. The settled fix is a held keeper session per class + unlink the socket file at tearDownClass — never a retry. #694's 'duplicate session' is a different message. Also: tests/_tmuxreap.py's _listening is called BY ITS PRIVATE NAME from tests/test_the_suite_reaps_its_own_tmux_servers.py (9 call sites) — renaming it reddens the suite, and 'grep ... | head -30' hides that.
