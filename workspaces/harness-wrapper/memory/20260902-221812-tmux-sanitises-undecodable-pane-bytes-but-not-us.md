# tmux SANITISES undecodable pane bytes but NOT user options — measured on

_2026-09-02 22:18 · persistent_

tmux SANITISES undecodable pane bytes but NOT user options — measured on 3.7c for #828. A pane printing \377 is stored in tmux's own screen as U+FFFD, so 'capture-pane -p -e -N' hands charter valid UTF-8 under LANG=C.UTF-8 AND under LC_ALL=C; the issue's 'a harness pane holds arbitrary bytes' repro path does NOT reach charter. What DOES: (1) a user option round-trips its bytes untouched — set-option -w @charter_chat with a raw \377 comes back out of "list-windows -a -F '#{@charter_chat}'" and display-message -p byte for byte, and that listing is _chat_seats, which cmd_quit asks before it kills anything; (2) tmux's own stderr echoes the raw bytes of an argument it refuses ('invalid window name: BAD\377NAME'), which is report_failure's input. So the decode defect really does land in a quit — one call to the LEFT of where the issue put it.
