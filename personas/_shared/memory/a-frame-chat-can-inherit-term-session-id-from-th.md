# A frame chat can inherit TERM_SESSION_ID from the terminal that launched

_2026-09-10 22:38 · persistent_

A frame chat can inherit TERM_SESSION_ID from the terminal that launched charter. _frame_env strips only TMUX, TMUX_PANE and the size variables, and session.py ranks TERM_SESSION_ID first when it derives a terminal id. So when charter claude is launched from Terminal.app or iTerm2 without tmux, anything a chat writes keyed on the terminal id (workspace use, persona use) lands on the LAUNCHING terminal's pointer: that terminal's next launch opens in the chat's choice, and other chats can resolve it through the terminal rung. This is the #411 shape; frame/switch.py avoids it by calling set_active with terminal_id set to the empty string. A test that mocks _terminal_id cannot see it. Measured by the PR 939 round-3 reviewer, 2026-09-10.
