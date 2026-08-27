# Dispatching several agents that each parallelize their own deletion swee

_2026-08-27 06:08 · persistent_

Dispatching several agents that each parallelize their own deletion sweep oversubscribes the machine badly: 2026-08-27 hit load 152 on 14 cores with 17 concurrent full-suite runs. Memory was fine (47% free, python RSS 0.2GB total) so it is CPU thrashing, not danger — the sweeps still finish, just far slower than serial. Agents notice and self-throttle if told the machine is loaded. Lesson: when dispatching more than one agent whose brief includes 'run the FULL suite per mutation', tell each one the others exist and not to raise concurrency to compensate.
