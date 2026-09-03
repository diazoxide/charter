# CHARTER PAINTS 'dim' MORE THAN ANYTHING ELSE, AND dim IS THE READABILITY

_2026-08-31 00:50 · persistent_

CHARTER PAINTS 'dim' MORE THAN ANYTHING ELSE, AND dim IS THE READABILITY BUG ON A LIGHT TERMINAL. Measured 2026-08-31 off the operator's own live frame with 'capture-pane -e' on socket charter: one pane carried ESC[2m twelve times, against green(32) x8, magenta(35) x1, blue(34) x1; a second pane ESC[2m x8 against yellow(33) x3. No bright codes (92/94/95) appeared at all — statusline._PALETTE only reaches them once enough personas cycle that far. So when an operator reports unreadable text on a bright/tan ground, 'dim' is the first thing to test, not the accent colours, and not the default foreground: on a terminal that is ALREADY light-ground, setting 'text' via window-style fg= is close to a no-op because their foreground is already dark. The accent triple (green/yellow/magenta on tan) is a SEPARATE and still-open gap — yellow-on-tan is the classic case.
