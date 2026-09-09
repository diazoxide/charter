# Confirmation surfaces size themselves to their blast radius (#927): chat

_2026-09-07 20:08 · persistent_

Confirmation surfaces size themselves to their blast radius (#927): chat: close is a five-row drawer, charter: quit keeps the whole pane. commands_frame._as_a_drawer decides it in one expression by reading the verb off the surface's own label (leave.CLOSE/leave.QUIT), because both call sites already spell it — so the rule lives in one place and frame/tabmenu inherits it. Same guard as leave.open_rows' placement rule (destructive rows last): distance between operator and a destructive answer scales with what the answer costs. Pin BOTH directions or the un-pinned one drifts silently — #921 had a passing test asserting quit was a drawer, and that assertion was the thing #927 had to change.
