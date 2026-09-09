# A frame integration fixture that does not set @charter_chat on each chat

_2026-09-08 17:09 · persistent_

A frame integration fixture that does not set @charter_chat on each chat window makes commands_frame._chat_seats report nothing, so leave.plan marks every chat live=False and leave.confirm_rows draws its NOTHING_OPEN row with NO confirming row. Every assertion that only checks the confirmation's heading ('close') then passes against a surface that names no chat at all — which is how a close confirmation could have named the wrong chat and stayed green. A two-chat close/quit fixture must set the window option, and must assert the chat BY NAME on the row and then follow it to state.was_closed and the window's death.
