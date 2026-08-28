# A tmux style value is NOT validated the way you'd hope: 'set -w pane-bor

_2026-08-28 20:55 · persistent_

A tmux style value is NOT validated the way you'd hope: 'set -w pane-border-style bg=#{?#{==:1,1},colour196,colour46}' is rc 0 and reads back verbatim (format-expanded at draw time), and 'bg=chartreuse' is rc 0 too because tmux knows the X11 colour names — a fixed RGB point no theme moves. Only a genuinely unknown word ('bg=notacolour') is rc 1 'invalid style:'. So tmux refusing a value is never the containment boundary; charter's closed word->constant tables are.
