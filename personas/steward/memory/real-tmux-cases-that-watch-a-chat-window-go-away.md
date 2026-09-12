# Real-tmux cases that watch a chat window go away need TWO assertions and

_2026-09-12 21:18 · persistent_

Real-tmux cases that watch a chat window go away need TWO assertions and a 30 s clock, not one and 20 s. Measured on CI 2026-09-12 across three runs of tests/test_a_new_chat_starts_at_the_profile_selector.TheSelectorOnARealServer: the Linux runner was red twice — once on the pane's exit CODE (a pane that exits out of raw mode there comes back dead with an empty pane_dead_status, which is commands_frame._UNKNOWN_DEATH_CODE's own measurement and ruling 42's) and once on 'the session outlived its only window' at a 20 s _eventually. Split into 'the selector left' then 'the window went', with 30 s and tmux's own answers (pane_dead_status, remain-on-exit, show-hooks -p) in the failure messages, all three runs are green. The feature was never wrong; the case was asking two claims as one on too short a clock.
