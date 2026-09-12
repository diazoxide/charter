# Never kill a process by name or pattern when sibling agents may be runni

_2026-09-10 17:06 · persistent_

Never kill a process by name or pattern when sibling agents may be running the same tool. On 2026-09-10 an implementer cleaned up with pkill -f matching tools/sweep.py, which matches every agent's sweep on the machine, not only its own. Kill the PID you started (record it when you launch a background run), or let the run finish. Put this rule into every parallel-agent brief: a subagent never sees the coordinator's memory.
