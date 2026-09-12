# Waiting on charter's full suite (~15-25 min) or tools/sweep.py (hours) f

_2026-09-11 21:18 · persistent_

Waiting on charter's full suite (~15-25 min) or tools/sweep.py (hours) from an agent session: never wait inside a foreground tool call. On a machine loaded by sibling agents (load 30-100, measured 2026-09-11) even a python loop of 3 x 55 s sleeps ran past the 240 s call timeout, and the stream watchdog stops the whole agent after 600 s of silence — it did, twice, killing an unfinished suite with it. The harness also refuses a shell 'sleep N; cmd' in the foreground. What works: start the run with run_in_background writing to a file, then arm a Monitor whose script prints only the lines to act on (the trailer, each non-pinned sweep result, a heartbeat every ~10 min) and exits when the runs end; the notifications wake the agent.
