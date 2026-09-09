# The outer terminal's mouse mode follows tmux's ACTIVE pane, not the zoom

_2026-09-07 18:45 · persistent_

The outer terminal's mouse mode follows tmux's ACTIVE pane, not the zoomed one — measured on 3.7c through a real pty: an unzoomed 5-row overlay pane that is still `active=1` has its \x1b[?1006h\x1b[?1000h propagated to the client. So zooming is never what makes a charter overlay's pointer work; `select-pane` is.
