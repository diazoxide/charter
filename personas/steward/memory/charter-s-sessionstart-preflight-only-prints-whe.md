# charter's SessionStart preflight only prints when doctor EXITS NON-ZERO

_2026-08-20 11:13 · persistent_

charter's SessionStart preflight only prints when doctor EXITS NON-ZERO — hooks/hooks.json runs 'out="$(charter doctor 2>&1)" || printf ...', and cmd_doctor exits 1 only on FAIL. So a doctor WARN reaches NOBODY in-session; it is visible only when someone runs 'charter doctor' by hand. The channel that does reach a session is hooks._pending_system -> systemMessage (renders at exit 0, blocks nothing), queued in _queue_plugin_notices at sessionstart only. Any future 'make doctor warn about X' request must answer 'who will ever see it?' first — issue #306 proposed exactly that remedy and it would have closed the issue while leaving the symptom (fixed properly in PR 307).
