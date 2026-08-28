# Never leave a daemon thread blocked in os.read(fd) when something else c

_2026-08-29 02:31 · persistent_

Never leave a daemon thread blocked in os.read(fd) when something else closes that fd: close does not interrupt the blocked read, and the fd NUMBER is then reused by the next subprocess.run pipe, so the thread silently eats that subprocess's stdout. Symptom seen: tmux display-message returned rc 0 with empty output, failing an unrelated test's teardown. Measured that tmux itself never answers rc 0 with an empty #{socket_path}.
