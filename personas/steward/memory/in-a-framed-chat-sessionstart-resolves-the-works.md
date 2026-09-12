# In a FRAMED chat, SessionStart resolves the workspace with the harness s

_2026-09-10 11:29 · persistent_

In a FRAMED chat, SessionStart resolves the workspace with the harness session id instead of the chat id (CHARTER_SESSION_ID), so it briefs the chat for default and tells you to run charter workspace use. Do NOT follow that nudge in a framed chat: the chat has no lock unless it was picked at launch, so ws use succeeds and splits the chat's CLI from its tab. Check with charter workspace list (it says via frame). To start work in another workspace, create it WITHOUT --use and open a chat there from the frame. Filed as diazoxide/charter#936 (2026-09-10).
