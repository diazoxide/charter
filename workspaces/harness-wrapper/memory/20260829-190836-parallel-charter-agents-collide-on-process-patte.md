# Parallel charter agents collide on process patterns, not just scratch fi

_2026-08-29 19:08 · persistent_

Parallel charter agents collide on process patterns, not just scratch files: 'pkill -f tools/sweep.py' and 'pkill -f "unittest discover -s tests"' kill the OTHER agent's runs too, and mine was killed the same way mid-trace. Never pkill by pattern when other agents are working — kill by the PID you started, and pass tools/sweep.py --workdir <namespaced path> so the two sandboxes cannot share a directory either.
