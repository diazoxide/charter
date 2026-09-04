# tmux's default MouseDown3Pane forwarding branch is '{ select-pane -t = ;

_2026-09-03 12:41 · persistent_

tmux's default MouseDown3Pane forwarding branch is '{ select-pane -t = ; send-keys -M }' — it SELECTS the pane before forwarding, so with [frame] mouse = true a right-click on any charter panel moves the keyboard off the harness. That is #634 one button over; measured on 3.7c via 'list-keys -T root' and reproduced end to end in tests/test_a_real_click_on_a_real_tab_bar_switches. It is pre-existing and NOT fixed by #846/PR847, because button 3's else-branch is a page-long display-menu that differs between supported tmux versions — filed as #848.
