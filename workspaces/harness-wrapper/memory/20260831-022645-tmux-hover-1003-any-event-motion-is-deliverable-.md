# tmux hover (1003 any-event motion) IS deliverable to a non-active charte

_2026-08-31 02:26 · persistent_

tmux hover (1003 any-event motion) IS deliverable to a non-active charter panel pane, measured on tmux 3.7c and the 3.2 floor with a real client on a real pty: (1) with tmux 'mouse on', a NON-active pane asking \x1b[?1003h makes tmux propagate 1003h to the OUTER terminal, and motion reports (SGR button 35) then arrive at that pane, pane-relative and 1-based; (2) motion over the HARNESS pane reaches nobody — tmux filters per-pane by what each pane asked for, so a pane asking only 1000 gets no motion. Hover therefore cannot poison the harness. (3) With tmux 'mouse off' the panel's 1003 request is NOT propagated (only the ACTIVE pane's request is), so hover requires [frame] mouse = true. Correction to a common assumption: 1003 does NOT wake every panel process — only the pane under the pointer that asked for it.
