# state.workspace_for(fid) reads $CHARTER_WORKSPACE out of the CALLING pro

_2026-09-04 12:33 · persistent_

state.workspace_for(fid) reads $CHARTER_WORKSPACE out of the CALLING process's environment (rung 0), so it answers for the frame only when a hook inside that frame asks. Any out-of-frame fan-out — a CLI command walking the frame root — must use state.own_workspace(fid), the frame's own recorded answer; workspace_for would file every chat on the plane under whatever the operator had exported and overwrite each cache with that one workspace's repos (#886).
