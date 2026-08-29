# tmux forwards SGR mouse buttons 128-131 (a mouse's thumb buttons) to a p

_2026-08-29 02:30 · persistent_

tmux forwards SGR mouse buttons 128-131 (a mouse's thumb buttons) to a pane verbatim, measured on 3.7c and 3.2. An xterm SGR button number is spread over THREE bit positions: (b & 3) + 4 if bit 6 + 8 if bit 7, with bit 5 meaning motion. Reading only the low two bits makes button 8 a 'left' click.
