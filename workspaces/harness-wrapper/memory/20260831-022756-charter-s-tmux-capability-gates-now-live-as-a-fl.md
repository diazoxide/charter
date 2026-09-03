# charter's tmux capability gates now live as a FLOOR FIELD on commands_fr

_2026-08-31 02:27 · persistent_

charter's tmux capability gates now live as a FLOOR FIELD on commands_frame._CHROME (option, value, first-tmux-that-has-it), not as a per-option check — #716 was the third time a capability shipped ungated (after PANE_BORDER_FLOOR and RESIZE_HOOK_FLOOR). Measured facts: pane-border-indicators arrived in tmux 3.3, so charter's own 3.2 floor printed 'styling the frame's own rules failed — invalid option' on EVERY launch; tmux 3.2 wraps its 'Pane is dead (…)' marker onto two rows where 3.7c truncates it to one, so any history-depth arithmetic over it is version-dependent (use capture-pane -S -). The 3.2 binary is at ~/.local/share/charter-testing/tmux-3.2 and CI installs no tmux at all, so all 95 real-tmux tests skip in CI and a green gate proves nothing about the floor — hand-run on both binaries.
