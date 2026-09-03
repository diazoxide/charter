# macOS has no 'timeout' command. 'timeout N python3 -m unittest discover

_2026-09-02 23:35 · persistent_

macOS has no 'timeout' command. 'timeout N python3 -m unittest discover -s tests' exits 0 with 'command not found' — a false green that looks exactly like a passing suite in a backgrounded job. Never wrap the CI-parity test command in 'timeout'; run it bare and read the trailer (OK / FAILED), not the exit code of the pipeline.
