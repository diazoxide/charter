# opencode 1.18.23's bundle applies a plugin's shell.env output as {...pro

_2026-09-10 15:13 · persistent_

opencode 1.18.23's bundle applies a plugin's shell.env output as {...process.env, ...b.env} (ShellTool.shellEnv) and, on the bash path, env:{...Y.env,TERM:'dumb'} with extendEnv:true — so a plugin's env value OVERRIDES the inherited environment, it does not merely fill gaps. Charter's generated opencode shim therefore replaces $CHARTER_SESSION_ID (the frame's chat id) with opencode's own session id in every shell, and forces it explicitly in the hook subprocess env with '?? ""' so an absent sessionID blanks it. Verified by strings on the installed binary, not from docs; filed as #946.
