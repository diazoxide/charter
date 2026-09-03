# tmux has NO command that suppresses paints while a batch applies: refres

_2026-09-02 15:55 · persistent_

tmux has NO command that suppresses paints while a batch applies: refresh-client forces a redraw, its -A pane:off is control-mode only, and suspend-client SIGTSTPs the client process. The only lever on flicker is fewer command LISTS — tmux redraws once per list, not once per command, measured as 45 client-repaints for 58 invocations and 14 for 23.
