# A charter frame is laid out on ONE chat at a time: cmd_chat step 2 is _a

_2026-09-08 17:09 · persistent_

A charter frame is laid out on ONE chat at a time: cmd_chat step 2 is _apply_arrangement(<chat being left>, want=[]), so every chat the operator is NOT on is a tmux window holding its harness pane and nothing else — state.panes(<that chat>) is literally {}. That is the correct resting state and it is invisible, because charter is what moves the client between chats and lays the entered one out on the way in. Anything that moves the client WITHOUT going through cmd_chat — a kill-window, tmux picking the next window itself — drops them on a frameless window, and an operator reads a bare harness pane as 'charter exited'. Verified on a live plane: the chat not on screen recorded panes: {} while its window was alive.
